//! SPIKE (M1, DESIGN.md §14 #4) – throwaway code, not for main.
//! Bridges a stdio language server to the frontend via a Tauri channel.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Channel;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;

use crate::commands::CommandError;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LspEvent {
    Message { payload: Value },
    Exited { code: Option<i32> },
}

struct Session {
    child: Child,
    stdin: ChildStdin,
}

#[derive(Default)]
pub struct LspSessions {
    next: AtomicU32,
    sessions: Mutex<HashMap<u32, Session>>,
}

fn err(code: &str, message: impl Into<String>) -> CommandError {
    CommandError {
        code: code.to_owned(),
        message: message.into(),
        hint: None,
    }
}

/// Reads one Content-Length framed message. Returns Ok(None) on EOF.
pub async fn read_frame<R: AsyncBufReadExt + Unpin>(r: &mut R) -> std::io::Result<Option<Vec<u8>>> {
    let mut len: Option<usize> = None;
    let mut line = String::new();
    loop {
        line.clear();
        if r.read_line(&mut line).await? == 0 {
            return Ok(None);
        }
        let l = line.trim_end_matches(['\r', '\n']);
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':')
            && k.trim().eq_ignore_ascii_case("content-length")
        {
            len = v.trim().parse().ok();
        }
    }
    let len = len.ok_or_else(|| std::io::Error::other("missing Content-Length"))?;
    let mut buf = vec![0; len];
    r.read_exact(&mut buf).await?;
    Ok(Some(buf))
}

pub fn frame(body: &[u8]) -> Vec<u8> {
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(body);
    out
}

#[tauri::command]
pub async fn spike_lsp_start(
    state: tauri::State<'_, LspSessions>,
    root: String,
    on_event: Channel<LspEvent>,
) -> Result<u32, CommandError> {
    let exe = std::env::var("IAC_SPIKE_TERRAFORM_LS")
        .map_err(|_| err("config", "IAC_SPIKE_TERRAFORM_LS not set"))?;
    let t0 = Instant::now();
    let mut child = Command::new(&exe)
        .arg("serve")
        .current_dir(&root)
        .env_clear()
        .envs(
            std::env::vars()
                .filter(|(k, _)| ["HOME", "PATH", "LANG", "TMPDIR"].contains(&k.as_str())),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| err("spawn", e.to_string()))?;
    let stdin = child.stdin.take().ok_or_else(|| err("spawn", "no stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| err("spawn", "no stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| err("spawn", "no stderr"))?;
    let id = state.next.fetch_add(1, Ordering::Relaxed) + 1;
    tracing::info!(
        id,
        pid = child.id(),
        spawn_ms = t0.elapsed().as_millis() as u64,
        "lsp started"
    );

    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            tracing::debug!(target: "lsp_stderr", "{l}");
        }
    });

    let channel = on_event.clone();
    tauri::async_runtime::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut count = 0u64;
        loop {
            match read_frame(&mut reader).await {
                Ok(Some(body)) => match serde_json::from_slice::<Value>(&body) {
                    Ok(payload) => {
                        count += 1;
                        tracing::debug!(target: "lsp_trace", dir = "<-", method = ?payload.get("method"), id = ?payload.get("id"), "server->client");
                        if channel.send(LspEvent::Message { payload }).is_err() {
                            break;
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "invalid json from server"),
                },
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!(error = %e, "framing error");
                    break;
                }
            }
        }
        tracing::info!(id, messages = count, "lsp stdout closed");
        let _ = channel.send(LspEvent::Exited { code: None });
    });

    state
        .sessions
        .lock()
        .await
        .insert(id, Session { child, stdin });
    Ok(id)
}

#[tauri::command]
pub async fn spike_lsp_send(
    state: tauri::State<'_, LspSessions>,
    id: u32,
    message: Value,
) -> Result<(), CommandError> {
    tracing::debug!(target: "lsp_trace", dir = "->", method = ?message.get("method"), id = ?message.get("id"), "client->server");
    let body = serde_json::to_vec(&message).map_err(|e| err("json", e.to_string()))?;
    let mut sessions = state.sessions.lock().await;
    let s = sessions
        .get_mut(&id)
        .ok_or_else(|| err("no_session", "unknown session"))?;
    s.stdin
        .write_all(&frame(&body))
        .await
        .map_err(|e| err("io", e.to_string()))?;
    s.stdin.flush().await.map_err(|e| err("io", e.to_string()))
}

/// Waits for the server to exit (after the client sent shutdown/exit), kills it after a grace period.
async fn stop_session(mut s: Session, grace: Duration) -> Option<i32> {
    drop(s.stdin); // EOF on stdin
    match tokio::time::timeout(grace, s.child.wait()).await {
        Ok(Ok(status)) => status.code(),
        _ => {
            tracing::warn!("lsp did not exit in time, killing");
            let _ = s.child.kill().await;
            None
        }
    }
}

#[tauri::command]
pub async fn spike_lsp_stop(
    state: tauri::State<'_, LspSessions>,
    id: u32,
) -> Result<Option<i32>, CommandError> {
    let s = state.sessions.lock().await.remove(&id);
    let t0 = Instant::now();
    let code = match s {
        Some(s) => stop_session(s, Duration::from_secs(3)).await,
        None => return Err(err("no_session", "unknown session")),
    };
    tracing::info!(
        id,
        ?code,
        stop_ms = t0.elapsed().as_millis() as u64,
        "lsp stopped"
    );
    Ok(code)
}

pub async fn stop_all(state: &LspSessions) {
    let all: Vec<(u32, Session)> = state.sessions.lock().await.drain().collect();
    for (id, s) in all {
        let code = stop_session(s, Duration::from_secs(2)).await;
        tracing::info!(id, ?code, "lsp stopped on app exit");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn frames_roundtrip() {
        let mut data = frame(br#"{"a":1}"#);
        data.extend(frame("{\"b\":\"ä\"}".as_bytes()));
        let mut r = BufReader::new(&data[..]);
        assert_eq!(read_frame(&mut r).await.unwrap().unwrap(), br#"{"a":1}"#);
        assert_eq!(
            read_frame(&mut r).await.unwrap().unwrap(),
            "{\"b\":\"ä\"}".as_bytes()
        );
        assert!(read_frame(&mut r).await.unwrap().is_none());
    }
}

#[tauri::command]
pub fn spike_log(line: String) {
    tracing::info!(target: "spike_frontend", "{line}");
}

/// Closes the main window like a user would (spike automation for the shutdown test).
#[tauri::command]
pub fn spike_close_window(app: tauri::AppHandle) {
    use tauri::Manager;
    if std::env::var("IAC_SPIKE_AUTOCLOSE").is_ok()
        && let Some(w) = app.get_webview_window("main")
    {
        tracing::info!("spike: closing main window");
        let _ = w.close();
    }
}
