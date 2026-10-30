//! `runner` (DESIGN.md section 4.3), M1 basic version: start a process with an
//! argument array and an allowlisted environment, stream its output line by line,
//! cancel it step by step. Run IDs, persistence and parsers come with M2.
//!
//! This is the only place in the app that starts child processes.

mod env;
mod lines;

use std::process::Stdio;
use std::time::Duration;

use camino::Utf8PathBuf;
use serde::Serialize;
use thiserror::Error;
use tokio::io::AsyncRead;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};
use ts_rs::TS;

pub use env::ChildEnv;

use crate::platform::{self, ProcessSignal};

/// Waiting times between the stop signals.
#[derive(Debug, Clone, Copy)]
pub struct StopTimeouts {
    pub after_interrupt: Duration,
    pub after_terminate: Duration,
}

impl Default for StopTimeouts {
    fn default() -> Self {
        Self {
            after_interrupt: Duration::from_secs(10),
            after_terminate: Duration::from_secs(5),
        }
    }
}

/// How long to keep reading output after the main process exited (descendants
/// may still hold the pipes open) before they are killed.
const DRAIN_AFTER_EXIT: Duration = Duration::from_secs(2);

/// Retries when exec fails with ETXTBSY (see `start`).
const SPAWN_BUSY_RETRIES: u32 = 10;
const SPAWN_BUSY_DELAY: Duration = Duration::from_millis(20);

#[derive(Debug, Clone)]
pub struct RunSpec {
    /// Absolute path of the executable (no `PATH` lookup).
    pub program: Utf8PathBuf,
    pub args: Vec<String>,
    pub cwd: Utf8PathBuf,
    pub env: ChildEnv,
    pub stop: StopTimeouts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum RunEvent {
    Started {
        pid: u32,
    },
    Line {
        stream: OutputStream,
        text: String,
        /// The line exceeded the length limit and was cut.
        truncated: bool,
    },
    Exited(RunOutcome),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct RunOutcome {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,
    /// The run was stopped on request.
    pub cancelled: bool,
}

impl RunOutcome {
    pub fn success(&self) -> bool {
        self.code == Some(0) && !self.cancelled
    }
}

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("program path must be absolute: {0}")]
    RelativeProgram(String),
    #[error("could not start {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("waiting for the process failed: {0}")]
    Wait(#[source] std::io::Error),
}

/// Handle of a running process.
pub struct RunHandle {
    pid: u32,
    cancel: Option<oneshot::Sender<()>>,
    done: tokio::task::JoinHandle<Result<RunOutcome, RunnerError>>,
}

impl RunHandle {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Requests a stop: SIGINT, then SIGTERM, then SIGKILL (to the whole tree).
    pub fn cancel(&mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(());
        }
    }

    pub async fn wait(self) -> Result<RunOutcome, RunnerError> {
        self.done
            .await
            .map_err(|e| RunnerError::Wait(std::io::Error::other(e.to_string())))?
    }
}

fn start(spec: &RunSpec, stdin_piped: bool) -> Result<Child, RunnerError> {
    if !spec.program.is_absolute() {
        return Err(RunnerError::RelativeProgram(spec.program.to_string()));
    }
    let mut command = Command::new(spec.program.as_std_path());
    command
        .args(&spec.args)
        .current_dir(spec.cwd.as_std_path())
        .env_clear()
        .envs(spec.env.vars())
        .stdin(if stdin_piped {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    platform::current().prepare_command(&mut command);

    // ETXTBSY: a freshly written executable (e.g. a just installed tool) can be
    // briefly "busy" because another thread forked while its write handle was
    // open; the child closes it on exec, so a short retry succeeds.
    let mut attempt = 0;
    let child = loop {
        match command.spawn() {
            Err(e)
                if e.kind() == std::io::ErrorKind::ExecutableFileBusy
                    && attempt < SPAWN_BUSY_RETRIES =>
            {
                attempt += 1;
                std::thread::sleep(SPAWN_BUSY_DELAY);
            }
            result => {
                break result.map_err(|source| RunnerError::Spawn {
                    program: spec.program.to_string(),
                    source,
                })?;
            }
        }
    };
    tracing::debug!(pid = child.id(), program = %spec.program, args = spec.args.len(), "process started");
    Ok(child)
}

/// Starts `spec`; events (including the final `Exited`) are sent to `events`.
pub fn spawn(spec: RunSpec, events: mpsc::Sender<RunEvent>) -> Result<RunHandle, RunnerError> {
    let mut child = start(&spec, false)?;
    let pid = child.id().unwrap_or(0);
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let done = tokio::spawn(supervise(
        child, pid, stdout, stderr, spec.stop, cancel_rx, events,
    ));
    Ok(RunHandle {
        pid,
        cancel: Some(cancel_tx),
        done,
    })
}

/// A process whose stdin/stdout are used as a byte stream by the caller
/// (language servers: JSON-RPC framing). stderr is still streamed as `Line`
/// events, the exit as `Exited`.
pub struct StdioProcess {
    pub handle: RunHandle,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
}

/// Like [`spawn`], but hands stdin and stdout to the caller.
pub fn spawn_stdio(
    spec: RunSpec,
    events: mpsc::Sender<RunEvent>,
) -> Result<StdioProcess, RunnerError> {
    let mut child = start(&spec, true)?;
    let pid = child.id().unwrap_or(0);
    let missing = |what: &str| RunnerError::Spawn {
        program: spec.program.to_string(),
        source: std::io::Error::other(format!("{what} not captured")),
    };
    let stdin = child.stdin.take().ok_or_else(|| missing("stdin"))?;
    let stdout = child.stdout.take().ok_or_else(|| missing("stdout"))?;
    let stderr = child.stderr.take();
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let done = tokio::spawn(supervise(
        child,
        pid,
        None::<ChildStdout>,
        stderr,
        spec.stop,
        cancel_rx,
        events,
    ));
    Ok(StdioProcess {
        handle: RunHandle {
            pid,
            cancel: Some(cancel_tx),
            done,
        },
        stdin,
        stdout,
    })
}

async fn supervise(
    mut child: Child,
    pid: u32,
    stdout: Option<impl AsyncRead + Unpin + Send + 'static>,
    stderr: Option<impl AsyncRead + Unpin + Send + 'static>,
    stop: StopTimeouts,
    mut cancel_rx: oneshot::Receiver<()>,
    events: mpsc::Sender<RunEvent>,
) -> Result<RunOutcome, RunnerError> {
    let _ = events.send(RunEvent::Started { pid }).await;
    let mut readers = Vec::new();
    if let Some(out) = stdout {
        readers.push(tokio::spawn(lines::forward(
            out,
            OutputStream::Stdout,
            events.clone(),
        )));
    }
    if let Some(err) = stderr {
        readers.push(tokio::spawn(lines::forward(
            err,
            OutputStream::Stderr,
            events.clone(),
        )));
    }

    let mut cancelled = false;
    let status = tokio::select! {
        status = child.wait() => status,
        _ = &mut cancel_rx => {
            cancelled = true;
            stop_tree(&mut child, pid, stop).await
        }
    }
    .map_err(RunnerError::Wait)?;

    // Descendants may keep the pipes open; give them a moment, then kill the group.
    let drain = async {
        for r in &mut readers {
            let _ = r.await;
        }
    };
    if tokio::time::timeout(DRAIN_AFTER_EXIT, drain).await.is_err() {
        let _ = platform::current().signal_process_tree(pid, ProcessSignal::Kill);
        for r in readers {
            r.abort();
        }
    }

    let outcome = RunOutcome {
        code: status.code(),
        signal: platform::current().termination_signal(&status),
        cancelled,
    };
    tracing::debug!(pid, ?outcome, "process finished");
    let _ = events.send(RunEvent::Exited(outcome.clone())).await;
    Ok(outcome)
}

async fn stop_tree(
    child: &mut Child,
    pid: u32,
    stop: StopTimeouts,
) -> std::io::Result<std::process::ExitStatus> {
    let p = platform::current();
    for (signal, grace) in [
        (ProcessSignal::Interrupt, Some(stop.after_interrupt)),
        (ProcessSignal::Terminate, Some(stop.after_terminate)),
        (ProcessSignal::Kill, None),
    ] {
        tracing::debug!(pid, ?signal, "stopping process");
        if let Err(e) = p.signal_process_tree(pid, signal) {
            tracing::debug!(pid, error = %e, "signal failed (process may be gone)");
        }
        match grace {
            Some(grace) => {
                if let Ok(status) = tokio::time::timeout(grace, child.wait()).await {
                    return status;
                }
            }
            None => return child.wait().await,
        }
    }
    child.wait().await
}

#[cfg(test)]
mod tests;
