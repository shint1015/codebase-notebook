//! Filesystem watcher for local sources: changes trigger a debounced
//! re-index of the owning workspace, keeping the index fresh without manual
//! "Re-index" clicks.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};

use crate::domain::error::{DomainError, DomainResult};

const QUIET_PERIOD: Duration = Duration::from_millis(1500);
const POLL: Duration = Duration::from_millis(500);

type Roots = Arc<Mutex<Vec<(PathBuf, String)>>>;

pub struct SourceWatcher {
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    roots: Roots,
    trigger_tx: Sender<String>,
}

impl SourceWatcher {
    /// `on_change` receives a workspace id once its sources have been quiet
    /// for [`QUIET_PERIOD`] after a burst of file events.
    pub fn new(on_change: Arc<dyn Fn(String) + Send + Sync>) -> Arc<Self> {
        let (tx, rx) = channel::<String>();
        std::thread::spawn(move || {
            let mut pending: HashMap<String, Instant> = HashMap::new();
            loop {
                match rx.recv_timeout(POLL) {
                    Ok(workspace_id) => {
                        pending.insert(workspace_id, Instant::now());
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                let now = Instant::now();
                let ready: Vec<String> = pending
                    .iter()
                    .filter(|(_, at)| now.duration_since(**at) >= QUIET_PERIOD)
                    .map(|(id, _)| id.clone())
                    .collect();
                for workspace_id in ready {
                    pending.remove(&workspace_id);
                    on_change(workspace_id);
                }
            }
        });
        Arc::new(Self {
            watcher: Mutex::new(None),
            roots: Arc::new(Mutex::new(Vec::new())),
            trigger_tx: tx,
        })
    }

    /// Replace the watched set with `targets` (root path, workspace id).
    /// Dropping the previous watcher releases its old watches.
    pub fn rebuild(&self, targets: Vec<(String, String)>) -> DomainResult<()> {
        *self.roots.lock().expect("roots lock") = targets
            .iter()
            .map(|(path, ws)| (PathBuf::from(path), ws.clone()))
            .collect();

        let roots = self.roots.clone();
        let tx = self.trigger_tx.clone();
        let mut watcher = notify::recommended_watcher(
            move |result: Result<notify::Event, notify::Error>| {
                let Ok(event) = result else { return };
                let roots = roots.lock().expect("roots lock");
                for path in &event.paths {
                    // Git internals churn constantly; user content does not
                    // live there.
                    if path.components().any(|c| c.as_os_str() == ".git") {
                        continue;
                    }
                    if let Some((_, workspace_id)) =
                        roots.iter().find(|(root, _)| path.starts_with(root))
                    {
                        tx.send(workspace_id.clone()).ok();
                        break;
                    }
                }
            },
        )
        .map_err(|e| DomainError::Indexing(format!("create watcher: {e}")))?;

        for (path, _) in &targets {
            // Missing paths (deleted folders) are skipped silently.
            watcher
                .watch(std::path::Path::new(path), RecursiveMode::Recursive)
                .ok();
        }
        *self.watcher.lock().expect("watcher lock") = Some(watcher);
        Ok(())
    }
}
