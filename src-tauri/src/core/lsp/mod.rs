//! `lsp` (DESIGN.md §4.6): language servers per project, bridged to the frontend.
//! Servers are started only for trusted projects (`TrustedProject`) and only
//! through `core/runner`.

pub mod framing;
pub mod servers;
pub mod session;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;
use serde_json::Value;
use thiserror::Error;

use crate::core::project::trust::TrustedProject;
pub use servers::{LaunchContext, ServerKind};
pub use session::{LspEvent, Session};

/// At most this many starts per project and server within `RESTART_WINDOW`
/// (the client restarts after crashes; this stops crash loops).
pub const MAX_STARTS: usize = 3;
pub const RESTART_WINDOW: Duration = Duration::from_secs(60);

pub type SessionId = u32;

#[derive(Debug, Error)]
pub enum LspError {
    #[error("{0} is not installed")]
    Unavailable(String),
    #[error("could not start the language server: {0}")]
    Spawn(String),
    #[error("the language server was started too often ({MAX_STARTS} times in 60 s)")]
    RestartLimit,
    #[error("unknown language server session")]
    UnknownSession,
    #[error("the language server has stopped")]
    Stopped,
}

impl LspError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unavailable(_) => "lsp_unavailable",
            Self::Spawn(_) => "lsp_spawn",
            Self::RestartLimit => "lsp_restart_limit",
            Self::UnknownSession => "lsp_unknown_session",
            Self::Stopped => "lsp_stopped",
        }
    }

    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::Unavailable(_) => Some("Set up the tools (status bar → Set up tools)."),
            Self::RestartLimit => Some("Check the server output; reopen the project to try again."),
            _ => None,
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
pub struct LspManager {
    next: AtomicU32,
    sessions: Mutex<HashMap<SessionId, Arc<Session>>>,
    starts: Mutex<HashMap<(Utf8PathBuf, ServerKind), Vec<Instant>>>,
}

impl LspManager {
    /// Starts a server for a trusted project. `ctx` must describe that project.
    pub fn start(
        &self,
        trusted: TrustedProject<'_>,
        kind: ServerKind,
        ctx: &LaunchContext,
        emit: impl Fn(LspEvent) + Send + Sync + 'static,
    ) -> Result<SessionId, LspError> {
        debug_assert_eq!(trusted.project().root(), ctx.project_root);
        let spec = servers::run_spec(kind, ctx)?;
        self.check_restart_budget(&ctx.project_root, kind)?;
        let session = Session::start(kind, spec, ctx.venv.clone(), emit)?;
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        lock(&self.sessions).insert(id, session);
        Ok(id)
    }

    fn check_restart_budget(&self, root: &Utf8PathBuf, kind: ServerKind) -> Result<(), LspError> {
        let mut starts = lock(&self.starts);
        let list = starts.entry((root.clone(), kind)).or_default();
        let now = Instant::now();
        list.retain(|t| now.duration_since(*t) < RESTART_WINDOW);
        if list.len() >= MAX_STARTS {
            return Err(LspError::RestartLimit);
        }
        list.push(now);
        Ok(())
    }

    pub async fn send(&self, id: SessionId, msg: Value) -> Result<(), LspError> {
        let session = lock(&self.sessions)
            .get(&id)
            .cloned()
            .ok_or(LspError::UnknownSession)?;
        session.send(msg).await
    }

    pub async fn stop(&self, id: SessionId) -> Result<(), LspError> {
        let session = lock(&self.sessions)
            .remove(&id)
            .ok_or(LspError::UnknownSession)?;
        session.stop().await;
        Ok(())
    }

    pub async fn stop_all(&self) {
        let all: Vec<_> = lock(&self.sessions).drain().map(|(_, s)| s).collect();
        for s in all {
            s.stop().await;
        }
    }

    pub fn session_count(&self) -> usize {
        lock(&self.sessions).len()
    }
}

#[cfg(test)]
mod real_tests;
#[cfg(test)]
mod tests;
