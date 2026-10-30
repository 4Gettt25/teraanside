//! LSP base protocol framing: `Content-Length: N\r\n[other headers]\r\n\r\n<N bytes>`.
//! https://microsoft.github.io/language-server-protocol/specifications/base/0.9/specification/

use thiserror::Error;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Largest accepted message body (protects against memory exhaustion).
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_HEADER_LINE: usize = 1024;
const MAX_HEADERS: usize = 16;

#[derive(Debug, Error)]
pub enum FramingError {
    #[error("missing Content-Length header")]
    MissingLength,
    #[error("invalid header: {0}")]
    InvalidHeader(String),
    #[error("message of {0} bytes exceeds the limit")]
    TooLarge(usize),
    #[error("stream ended inside a message")]
    UnexpectedEof,
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
}

/// Reads one message body. `Ok(None)` on a clean end of stream between messages.
pub async fn read_message<R: AsyncBufRead + Unpin>(
    r: &mut R,
    max: usize,
) -> Result<Option<Vec<u8>>, FramingError> {
    let mut length: Option<usize> = None;
    let mut headers = 0;
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = (&mut *r)
            .take(MAX_HEADER_LINE as u64 + 1)
            .read_until(b'\n', &mut line)
            .await?;
        if n == 0 {
            return if headers == 0 {
                Ok(None)
            } else {
                Err(FramingError::UnexpectedEof)
            };
        }
        if n > MAX_HEADER_LINE || !line.ends_with(b"\n") {
            return Err(FramingError::InvalidHeader(
                "header line too long or unterminated".into(),
            ));
        }
        let text = std::str::from_utf8(&line)
            .map_err(|_| FramingError::InvalidHeader("non-UTF-8 header".into()))?
            .trim_end_matches(['\r', '\n']);
        if text.is_empty() {
            if headers == 0 {
                continue; // tolerate stray blank lines between messages
            }
            break;
        }
        headers += 1;
        if headers > MAX_HEADERS {
            return Err(FramingError::InvalidHeader("too many headers".into()));
        }
        let (name, value) = text
            .split_once(':')
            .ok_or_else(|| FramingError::InvalidHeader(text.chars().take(80).collect()))?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            let v: usize = value.trim().parse().map_err(|_| {
                FramingError::InvalidHeader(format!("Content-Length: {}", value.trim()))
            })?;
            length = Some(v);
        }
    }
    let length = length.ok_or(FramingError::MissingLength)?;
    if length > max {
        return Err(FramingError::TooLarge(length));
    }
    let mut body = vec![0; length];
    r.read_exact(&mut body).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            FramingError::UnexpectedEof
        } else {
            FramingError::Io(e)
        }
    })?;
    Ok(Some(body))
}

pub async fn write_message<W: AsyncWrite + Unpin>(w: &mut W, body: &[u8]) -> std::io::Result<()> {
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    w.write_all(header.as_bytes()).await?;
    w.write_all(body).await?;
    w.flush().await
}

#[cfg(test)]
mod tests {
    use tokio::io::BufReader;

    use super::*;

    async fn read_all(bytes: &[u8], max: usize) -> Result<Vec<Vec<u8>>, FramingError> {
        let mut r = BufReader::new(bytes);
        let mut out = Vec::new();
        while let Some(m) = read_message(&mut r, max).await? {
            out.push(m);
        }
        Ok(out)
    }

    #[tokio::test]
    async fn round_trip_with_unicode_and_extra_headers() {
        let mut buf = Vec::new();
        write_message(&mut buf, br#"{"a":1}"#).await.unwrap();
        write_message(&mut buf, "{\"t\":\"Grüße ✓\"}".as_bytes())
            .await
            .unwrap();
        buf.extend_from_slice(
            b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\ncontent-length: 2\r\n\r\n{}",
        );
        let msgs = read_all(&buf, MAX_MESSAGE_BYTES).await.unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[1], "{\"t\":\"Grüße ✓\"}".as_bytes());
        assert_eq!(msgs[2], b"{}");
    }

    #[tokio::test]
    async fn handles_messages_split_across_reads() {
        // a reader that returns one byte at a time
        let data = b"Content-Length: 5\r\n\r\nhelloContent-Length: 1\r\n\r\nx".to_vec();
        let (client, mut server) = tokio::io::duplex(1);
        tokio::spawn(async move {
            for b in data {
                server.write_all(&[b]).await.unwrap();
            }
        });
        let mut r = BufReader::new(client);
        assert_eq!(read_message(&mut r, 100).await.unwrap().unwrap(), b"hello");
        assert_eq!(read_message(&mut r, 100).await.unwrap().unwrap(), b"x");
        assert!(read_message(&mut r, 100).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn rejects_malformed_input() {
        let cases: &[(&[u8], &str)] = &[
            (b"Content-Type: x\r\n\r\n{}", "missing"),
            (b"Content-Length: abc\r\n\r\n", "invalid"),
            (b"Content-Length: -1\r\n\r\n", "invalid"),
            (b"no colon here\r\n\r\n", "invalid"),
            (b"Content-Length: 10\r\n\r\nshort", "eof"),
            (b"Content-Length: 99999999999\r\n\r\n", "large"),
        ];
        for (input, kind) in cases {
            let err = read_all(input, 1000).await.unwrap_err();
            let ok = match *kind {
                "missing" => matches!(err, FramingError::MissingLength),
                "invalid" => matches!(err, FramingError::InvalidHeader(_)),
                "eof" => matches!(err, FramingError::UnexpectedEof),
                _ => matches!(err, FramingError::TooLarge(_)),
            };
            assert!(ok, "{:?}: {err:?}", String::from_utf8_lossy(input));
        }
        let long = format!("X-Junk: {}\r\n", "a".repeat(2000));
        assert!(matches!(
            read_all(long.as_bytes(), 1000).await,
            Err(FramingError::InvalidHeader(_))
        ));
    }

    #[tokio::test]
    async fn eof_inside_headers_is_an_error_but_clean_eof_is_not() {
        assert!(read_all(b"", 10).await.unwrap().is_empty());
        assert!(matches!(
            read_all(b"Content-Length: 2\r\n", 10).await,
            Err(FramingError::UnexpectedEof)
        ));
    }
}
