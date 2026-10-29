use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::mpsc;

use super::{OutputStream, RunEvent};

/// Longest line forwarded as one event; longer lines are cut (rest dropped until
/// the next newline) so that a misbehaving tool cannot exhaust memory.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// Splits a byte stream into lines (`\n`, optional `\r`), lossy UTF-8.
pub(super) async fn forward(
    mut reader: impl AsyncRead + Unpin,
    stream: OutputStream,
    events: mpsc::Sender<RunEvent>,
) {
    let mut buf = vec![0u8; 64 * 1024];
    let mut line: Vec<u8> = Vec::new();
    let mut truncated = false;
    loop {
        let n = match reader.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for &b in &buf[..n] {
            if b == b'\n' {
                if !emit(&events, stream, &mut line, &mut truncated).await {
                    return;
                }
            } else if line.len() < MAX_LINE_BYTES {
                line.push(b);
            } else {
                truncated = true;
            }
        }
    }
    if !line.is_empty() || truncated {
        emit(&events, stream, &mut line, &mut truncated).await;
    }
}

async fn emit(
    events: &mpsc::Sender<RunEvent>,
    stream: OutputStream,
    line: &mut Vec<u8>,
    truncated: &mut bool,
) -> bool {
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    let text = String::from_utf8_lossy(line).into_owned();
    line.clear();
    let event = RunEvent::Line {
        stream,
        text,
        truncated: std::mem::take(truncated),
    };
    events.send(event).await.is_ok()
}
