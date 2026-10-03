//! Universal search (the Spotlight-style bar): owns the in-memory index,
//! rebuilds it in the background, and shows/hides the search window.

use crate::state::AppState;
use serde::Serialize;
use sf_engine::search_index::{default_app_roots, default_file_roots, Entry, Index};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const MAX_ENTRIES: usize = 400_000;
const REFRESH_EVERY: Duration = Duration::from_secs(20 * 60);

#[derive(Default)]
pub struct Finder {
    index: RwLock<Arc<Index>>,
    building: AtomicBool,
    built_at: RwLock<Option<Instant>>,
}

#[derive(Serialize)]
pub struct FinderInfo {
    entries: usize,
    building: bool,
    roots: Vec<PathBuf>,
    seconds_since_build: Option<u64>,
}

impl Finder {
    pub fn query(&self, q: &str, limit: usize) -> Vec<Entry> {
        let idx = self.index.read().map(|i| i.clone()).unwrap_or_default();
        idx.query(q, limit)
    }
}

/// Folders searched: the user's choice, or the usual ones plus the Super
/// Folder and watch folders.
pub fn roots(app: &AppHandle) -> Vec<PathBuf> {
    let g = app.state::<AppState>();
    let g = g.lock();
    let s = &g.cfg.settings;
    let mut roots = if s.search_roots.is_empty() {
        default_file_roots()
    } else {
        s.search_roots.clone()
    };
    if s.search_roots.is_empty() {
        roots.extend(s.super_folder.iter().cloned());
        roots.extend(s.watch_folders.iter().map(|w| w.path.clone()));
    }
    roots
}

/// Rebuild the index on a background thread (no-op if one is running).
pub fn rebuild(app: &AppHandle) {
    let finder = app.state::<Finder>();
    if finder.building.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let roots = roots(&app);
        let started = Instant::now();
        let index = Index::build(&roots, &default_app_roots(), MAX_ENTRIES);
        tracing::info!(
            "search index: {} entries in {:?}",
            index.len(),
            started.elapsed()
        );
        let finder = app.state::<Finder>();
        if let Ok(mut slot) = finder.index.write() {
            *slot = Arc::new(index);
        }
        if let Ok(mut t) = finder.built_at.write() {
            *t = Some(Instant::now());
        }
        finder.building.store(false, Ordering::SeqCst);
        let _ = app.emit("search-index-ready", ());
    });
}

/// Initial build plus a periodic refresh so new files show up.
pub fn start(app: &AppHandle) {
    rebuild(app);
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(REFRESH_EVERY);
        rebuild(&app);
    });
}

pub fn info(app: &AppHandle) -> FinderInfo {
    let f = app.state::<Finder>();
    FinderInfo {
        entries: f.index.read().map(|i| i.len()).unwrap_or(0),
        building: f.building.load(Ordering::SeqCst),
        roots: roots(app),
        seconds_since_build: f
            .built_at
            .read()
            .ok()
            .and_then(|t| t.map(|t| t.elapsed().as_secs())),
    }
}

/// Toggle the search window: show it centred and focused, or hide it.
pub fn toggle(app: &AppHandle) {
    let Some(w) = app.get_webview_window("spotlight") else {
        return;
    };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    let _ = w.center();
    let _ = w.show();
    let _ = w.set_focus();
    let _ = app.emit_to("spotlight", "spotlight-shown", ());
}
