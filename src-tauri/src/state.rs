//! App state shared by commands, the watcher thread and the tray.
//!
//! One mutex over everything: operations are short and the app is
//! single-user, so simplicity beats lock granularity here.

use serde::Serialize;
use sf_engine::watcher::WatchHandle;
use sf_engine::Config;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

pub struct AppState(Mutex<Inner>);

pub struct Inner {
    pub cfg: Config,
    /// Feeds history op ids (engine contract: a shared counter).
    pub counter: u64,
    /// Files in the Super Folder waiting for a confirmed organize.
    pub pending_super: Vec<PathBuf>,
    /// Files in watch folders waiting for a confirmed organize.
    pub pending_watch: Vec<PathBuf>,
    /// Files that could not be organized (no rule, conflict skip, failure).
    pub attention: Vec<Attention>,
    pub watcher: Option<WatchHandle>,
    pub watcher_error: Option<String>,
    /// Paths from `--organize` (right-click menu) waiting for the window.
    pub startup_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Attention {
    pub path: PathBuf,
    pub reason: String,
}

impl AppState {
    pub fn new(cfg: Config) -> Self {
        AppState(Mutex::new(Inner {
            cfg,
            counter: 0,
            pending_super: Vec::new(),
            pending_watch: Vec::new(),
            attention: Vec::new(),
            watcher: None,
            watcher_error: None,
            startup_paths: Vec::new(),
        }))
    }

    /// A panicked holder must not brick the app: recover the data.
    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
}

impl Inner {
    pub fn add_attention(&mut self, path: PathBuf, reason: String) {
        self.attention.retain(|a| a.path != path);
        self.attention.push(Attention { path, reason });
        // Keep the list small; it is a to-do list, not a log.
        if self.attention.len() > 500 {
            let extra = self.attention.len() - 500;
            self.attention.drain(..extra);
        }
    }
}
