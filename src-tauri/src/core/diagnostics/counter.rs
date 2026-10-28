use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::core::CoreError;

pub const MAX_COUNT: u32 = 10_000;
pub const MAX_INTERVAL_MS: u32 = 10_000;

/// Parameters of a demo counter stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CounterRequest {
    /// Number of ticks to emit (1..=10000).
    pub count: u32,
    /// Delay between ticks in milliseconds (0..=10000).
    pub interval_ms: u32,
}

impl CounterRequest {
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.count == 0 || self.count > MAX_COUNT {
            return Err(CoreError::InvalidInput(format!(
                "count must be between 1 and {MAX_COUNT}"
            )));
        }
        if self.interval_ms > MAX_INTERVAL_MS {
            return Err(CoreError::InvalidInput(format!(
                "intervalMs must be at most {MAX_INTERVAL_MS}"
            )));
        }
        Ok(())
    }
}

/// Events of the demo counter stream, used to verify streaming end-to-end (M0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum CounterEvent {
    Started { total: u32 },
    Tick { value: u32 },
    Finished { total: u32 },
}

/// Emits `Started`, `count` x `Tick` (1-based) and `Finished` through `emit`.
/// Stops with `CoreError::StreamClosed` if `emit` fails.
pub async fn run_counter<F>(request: CounterRequest, mut emit: F) -> Result<(), CoreError>
where
    F: FnMut(CounterEvent) -> Result<(), CoreError>,
{
    request.validate()?;
    let interval = Duration::from_millis(u64::from(request.interval_ms));

    emit(CounterEvent::Started {
        total: request.count,
    })?;
    for value in 1..=request.count {
        if !interval.is_zero() {
            tokio::time::sleep(interval).await;
        }
        emit(CounterEvent::Tick { value })?;
    }
    emit(CounterEvent::Finished {
        total: request.count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(count: u32, interval_ms: u32) -> CounterRequest {
        CounterRequest { count, interval_ms }
    }

    #[tokio::test(start_paused = true)]
    async fn emits_started_ticks_finished_in_order() {
        let mut events = Vec::new();
        run_counter(req(3, 100), |e| {
            events.push(e);
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(
            events,
            vec![
                CounterEvent::Started { total: 3 },
                CounterEvent::Tick { value: 1 },
                CounterEvent::Tick { value: 2 },
                CounterEvent::Tick { value: 3 },
                CounterEvent::Finished { total: 3 },
            ]
        );
    }

    #[tokio::test]
    async fn rejects_invalid_requests_without_emitting() {
        for bad in [
            req(0, 0),
            req(MAX_COUNT + 1, 0),
            req(1, MAX_INTERVAL_MS + 1),
        ] {
            let mut emitted = 0;
            let err = run_counter(bad, |_| {
                emitted += 1;
                Ok(())
            })
            .await
            .unwrap_err();
            assert_eq!(err.code(), "invalid_input");
            assert_eq!(emitted, 0);
        }
    }

    #[tokio::test]
    async fn stops_when_stream_is_closed() {
        let mut emitted = 0;
        let err = run_counter(req(5, 0), |_| {
            emitted += 1;
            if emitted >= 2 {
                Err(CoreError::StreamClosed)
            } else {
                Ok(())
            }
        })
        .await
        .unwrap_err();
        assert_eq!(err.code(), "stream_closed");
        assert_eq!(emitted, 2);
    }

    #[test]
    fn serializes_as_tagged_camel_case() {
        let json = serde_json::to_string(&CounterEvent::Tick { value: 7 }).unwrap();
        assert_eq!(json, r#"{"type":"tick","value":7}"#);
    }
}
