//! One running language server: framing on its stdio, a bounded writer queue,
//! stderr tail for diagnostics, and the stop sequence (stdin EOF → grace →
//! SIGINT/SIGTERM/SIGKILL via the runner).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use camino::Utf8PathBuf;
use serde::Serialize;
use serde_json::Value;
use tokio::io::BufReader;
use tokio::sync::{mpsc, oneshot};
use ts_rs::TS;

use super::LspError;
use super::framing::{self, MAX_MESSAGE_BYTES};
use super::servers::{ServerKind, enforce_ansible_message};
use crate::core::runner::{self, RunEvent, RunOutcome, RunSpec};

const STDERR_TAIL: usize = 20;
const WRITE_QUEUE: usize = 256;
/// After stdin EOF the server should exit on its own (terraform-ls does, see §14.2).
const EXIT_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export, optional_fields)]
pub enum LspEvent {
    /// A JSON-RPC message from the server.
    Message {
        #[ts(type = "unknown")]
        payload: Value,
    },
    /// The server process ended (also after a requested stop).
    Exited {
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        signal: Option<i32>,
        /// Ended without a stop request.
        crashed: bool,
        /// Last stderr lines (for the error display).
        stderr_tail: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

type PendingConfig = Arc<Mutex<HashMap<String, Vec<Option<String>>>>>;

pub struct Session {
    pub kind: ServerKind,
    writer: Mutex<Option<mpsc::Sender<Value>>>,
    stop_tx: Mutex<Option<oneshot::Sender<()>>>,
    done: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Session {
    /// Starts the server. `emit` receives messages and the final `Exited`.
    pub fn start(
        kind: ServerKind,
        spec: RunSpec,
        venv: Option<Utf8PathBuf>,
        emit: impl Fn(LspEvent) + Send + Sync + 'static,
    ) -> Result<Arc<Self>, LspError> {
        let (run_tx, mut run_rx) = mpsc::channel::<RunEvent>(64);
        let proc = runner::spawn_stdio(spec, run_tx).map_err(|e| LspError::Spawn(e.to_string()))?;
        let pid = proc.handle.pid();
        tracing::info!(?kind, pid, "language server started");
        let emit = Arc::new(emit);
        let pending: PendingConfig = Arc::default();

        // server -> client
        let reader = {
            let (emit, pending) = (emit.clone(), pending.clone());
            let mut stdout = BufReader::new(proc.stdout);
            tokio::spawn(async move {
                loop {
                    match framing::read_message(&mut stdout, MAX_MESSAGE_BYTES).await {
                        Ok(Some(body)) => match serde_json::from_slice::<Value>(&body) {
                            Ok(payload) => {
                                tracing::trace!(?kind, method = ?payload.get("method"), id = ?payload.get("id"), "server -> client");
                                if kind == ServerKind::Ansible {
                                    remember_config_request(&payload, &pending);
                                }
                                emit(LspEvent::Message { payload });
                            }
                            Err(e) => return Some(format!("invalid JSON from server: {e}")),
                        },
                        Ok(None) => return None,
                        Err(e) => return Some(format!("framing error: {e}")),
                    }
                }
            })
        };

        // client -> server (bounded queue = backpressure)
        let (write_tx, mut write_rx) = mpsc::channel::<Value>(WRITE_QUEUE);
        let writer = {
            let mut stdin = proc.stdin;
            tokio::spawn(async move {
                while let Some(mut msg) = write_rx.recv().await {
                    if kind == ServerKind::Ansible
                        && let Some(venv) = &venv
                    {
                        let sections = msg
                            .get("id")
                            .filter(|_| msg.get("method").is_none())
                            .and_then(|id| lock(&pending).remove(&id.to_string()));
                        enforce_ansible_message(&mut msg, sections.as_deref(), venv);
                    }
                    let Ok(body) = serde_json::to_vec(&msg) else {
                        continue;
                    };
                    if framing::write_message(&mut stdin, &body).await.is_err() {
                        break; // server gone; the exit is reported by the supervisor
                    }
                }
                // dropping stdin = EOF for the server
            })
        };

        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let mut handle = proc.handle;
        let done = tokio::spawn(async move {
            let mut tail: VecDeque<String> = VecDeque::new();
            let mut stop_rx = stop_rx;
            let mut stopping = false;
            let mut outcome: Option<RunOutcome> = None;
            let mut reader = reader;
            let mut reader_error: Option<String> = None;
            let mut reader_done = false;
            loop {
                tokio::select! {
                    ev = run_rx.recv() => match ev {
                        Some(RunEvent::Line { text, .. }) => {
                            tracing::debug!(target: "lsp_stderr", ?kind, "{text}");
                            tail.push_back(text);
                            if tail.len() > STDERR_TAIL { tail.pop_front(); }
                        }
                        Some(RunEvent::Exited(o)) => { outcome = Some(o); break; }
                        Some(RunEvent::Started { .. }) => {}
                        None => break,
                    },
                    r = &mut reader, if !reader_done => {
                        reader_done = true;
                        reader_error = r.ok().flatten();
                        if let Some(err) = &reader_error {
                            tracing::warn!(?kind, error = %err, "stopping language server");
                            handle.cancel();
                        }
                    }
                    _ = &mut stop_rx, if !stopping => {
                        stopping = true;
                        // writer already dropped stdin; give the server time to exit
                        let stop_handle = &mut handle;
                        tokio::select! {
                            ev = wait_exit(&mut run_rx, &mut tail) => { outcome = ev; break; }
                            () = tokio::time::sleep(EXIT_GRACE) => stop_handle.cancel(),
                        }
                    }
                }
            }
            let outcome = match outcome {
                Some(o) => Some(o),
                None => handle.wait().await.ok(),
            };
            writer.abort();
            let code = outcome.as_ref().and_then(|o| o.code);
            let signal = outcome.as_ref().and_then(|o| o.signal);
            let crashed = !stopping && !(code == Some(0) && reader_error.is_none());
            tracing::info!(?kind, ?code, ?signal, crashed, "language server exited");
            emit(LspEvent::Exited {
                code,
                signal,
                crashed,
                stderr_tail: tail.into_iter().collect(),
                error: reader_error,
            });
        });

        Ok(Arc::new(Self {
            kind,
            writer: Mutex::new(Some(write_tx)),
            stop_tx: Mutex::new(Some(stop_tx)),
            done: Mutex::new(Some(done)),
        }))
    }

    /// Queues a client message (waits if the queue is full).
    pub async fn send(&self, msg: Value) -> Result<(), LspError> {
        let tx = lock(&self.writer).clone().ok_or(LspError::Stopped)?;
        tx.send(msg).await.map_err(|_| LspError::Stopped)
    }

    /// Closes stdin, waits for the exit (grace), then escalates. Idempotent.
    pub async fn stop(&self) {
        lock(&self.writer).take(); // writer task ends -> stdin EOF
        if let Some(tx) = lock(&self.stop_tx).take() {
            let _ = tx.send(());
        }
        let done = lock(&self.done).take();
        if let Some(done) = done {
            let _ = done.await;
        }
    }
}

async fn wait_exit(
    rx: &mut mpsc::Receiver<RunEvent>,
    tail: &mut VecDeque<String>,
) -> Option<RunOutcome> {
    while let Some(ev) = rx.recv().await {
        match ev {
            RunEvent::Exited(o) => return Some(o),
            RunEvent::Line { text, .. } => {
                tail.push_back(text);
                if tail.len() > STDERR_TAIL {
                    tail.pop_front();
                }
            }
            RunEvent::Started { .. } => {}
        }
    }
    None
}

/// Remembers the sections of a server's `workspace/configuration` request so the
/// client's response can be enforced (`servers::enforce_ansible_message`).
fn remember_config_request(payload: &Value, pending: &PendingConfig) {
    if payload.get("method").and_then(Value::as_str) != Some("workspace/configuration") {
        return;
    }
    let Some(id) = payload.get("id") else { return };
    let sections = payload
        .pointer("/params/items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|i| i.get("section").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let mut map = lock(pending);
    if map.len() > 1000 {
        map.clear(); // never answered; do not grow without bound
    }
    map.insert(id.to_string(), sections);
}
