//! Diagnostics used to verify the IPC path end-to-end (M0).

mod counter;
mod ping;

pub use counter::{CounterEvent, CounterRequest, run_counter};
pub use ping::{Pong, ping};
