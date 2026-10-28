//! External change notifications for an open project (notify + debounce).

use std::collections::BTreeSet;
use std::sync::mpsc;
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use super::ProjectError;

const DEBOUNCE: Duration = Duration::from_millis(250);
const IGNORED_PREFIXES: &[&str] = &[".git", ".terraform", "node_modules", ".iac-studio/runs"];

/// Keeps the OS watcher alive; dropping it stops watching.
pub struct ProjectWatcher {
    _watcher: RecommendedWatcher,
}

/// Starts watching `root` recursively (symlinks are not followed). `on_change` is
/// called from a background thread with the debounced, project-relative paths.
pub fn watch<F>(root: &Utf8Path, on_change: F) -> Result<ProjectWatcher, ProjectError>
where
    F: Fn(Vec<String>) + Send + 'static,
{
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let config = notify::Config::default().with_follow_symlinks(false);
    let mut watcher = RecommendedWatcher::new(tx, config).map_err(|e| watch_error(root, e))?;
    watcher
        .watch(root.as_std_path(), RecursiveMode::Recursive)
        .map_err(|e| watch_error(root, e))?;

    let root = root.to_owned();
    std::thread::Builder::new()
        .name("project-watch".into())
        .spawn(move || debounce_loop(&root, &rx, &on_change))
        .map_err(|e| ProjectError::Io {
            path: "watcher thread".into(),
            source: e,
        })?;
    Ok(ProjectWatcher { _watcher: watcher })
}

fn watch_error(root: &Utf8Path, e: notify::Error) -> ProjectError {
    ProjectError::Io {
        path: root.to_string(),
        source: std::io::Error::other(e.to_string()),
    }
}

fn debounce_loop(
    root: &Utf8Path,
    rx: &mpsc::Receiver<notify::Result<notify::Event>>,
    on_change: &dyn Fn(Vec<String>),
) {
    let mut pending = BTreeSet::new();
    loop {
        let event = if pending.is_empty() {
            rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        } else {
            rx.recv_timeout(DEBOUNCE)
        };
        match event {
            Ok(Ok(event)) => {
                for path in event.paths {
                    if let Some(rel) = relevant_relative(root, &path) {
                        pending.insert(rel);
                    }
                }
            }
            Ok(Err(e)) => tracing::warn!(error = %e, "file watcher error"),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                on_change(std::mem::take(&mut pending).into_iter().collect());
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

pub(crate) fn relevant_relative(root: &Utf8Path, path: &std::path::Path) -> Option<String> {
    let path = Utf8PathBuf::from_path_buf(path.to_path_buf()).ok()?;
    let rel = path.strip_prefix(root).ok()?;
    let rel = rel.as_str();
    if rel.is_empty()
        || IGNORED_PREFIXES
            .iter()
            .any(|p| rel == *p || rel.starts_with(&format!("{p}/")))
    {
        return None;
    }
    Some(rel.to_owned())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use super::*;

    #[test]
    fn filters_and_relativizes_paths() {
        let root = Utf8Path::new("/p");
        let rel = |s: &str| relevant_relative(root, std::path::Path::new(s));
        assert_eq!(rel("/p/main.tf"), Some("main.tf".into()));
        assert_eq!(
            rel("/p/roles/x/tasks/main.yml"),
            Some("roles/x/tasks/main.yml".into())
        );
        assert_eq!(rel("/p/.git/index"), None);
        assert_eq!(rel("/p/.terraform"), None);
        assert_eq!(rel("/p/.iac-studio/runs/1/x"), None);
        assert_eq!(
            rel("/p/.iac-studio/project.toml"),
            Some(".iac-studio/project.toml".into())
        );
        assert_eq!(rel("/other/x"), None);
        assert_eq!(rel("/p"), None);
    }

    #[test]
    fn reports_debounced_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(tmp.path().canonicalize().unwrap()).unwrap();
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = seen.clone();
        let _w = watch(&root, move |paths| sink.lock().unwrap().extend(paths)).unwrap();
        std::fs::write(root.join("a.tf"), "x").unwrap();
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::write(root.join(".git/HEAD"), "x").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && !seen.lock().unwrap().contains(&"a.tf".to_owned()) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let seen = seen.lock().unwrap();
        assert!(seen.contains(&"a.tf".to_owned()), "{seen:?}");
        assert!(seen.iter().all(|p| !p.starts_with(".git")), "{seen:?}");
    }
}
