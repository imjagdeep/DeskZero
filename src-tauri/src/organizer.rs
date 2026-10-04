//! Glue between the engine and the app: the background watcher, running a
//! plan, and the status that the window and tray show.

use crate::state::{AppState, Inner};
use crate::tray;
use serde::Serialize;
use sf_engine::mover::{self, AskResolution};
use sf_engine::types::{PlanMode, PlanStatus, PlannedOp, Settings};
use sf_engine::HistoryKind;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub organized_today: usize,
    pub needs_attention: usize,
    pub pending: usize,
    pub paused: bool,
    pub watching: Vec<PathBuf>,
    pub organize_root: Option<PathBuf>,
    pub watcher_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub moved: usize,
    pub failed: Vec<FailedDto>,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FailedDto {
    pub path: PathBuf,
    pub error: String,
}

/// Folders the watcher should observe: your DeskZero plus enabled
/// watch folders that exist.
pub fn watched_folders(settings: &Settings) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(sf) = &settings.organize_root {
        out.push(sf.clone());
    }
    for w in settings.watch_folders.iter().filter(|w| w.enabled) {
        out.push(w.path.clone());
    }
    out.retain(|p| p.is_dir());
    out
}

/// (Re)start the background watcher from the current settings. Paused or
/// nothing to watch means no watcher at all.
pub fn restart_watcher(app: &AppHandle) {
    let state = app.state::<AppState>();
    let moved;
    let notify;
    {
        let mut g = state.lock();
        if let Some(mut h) = g.watcher.take() {
            h.stop();
        }
        g.watcher_error = None;
        let folders = watched_folders(&g.cfg.settings);
        if !g.cfg.settings.paused && !folders.is_empty() {
            match sf_engine::watcher::start(&folders) {
                Ok((handle, rx)) => {
                    g.watcher = Some(handle);
                    let app2 = app.clone();
                    std::thread::spawn(move || {
                        // Ends when the watcher stops and drops its sender.
                        while let Ok(batch) = rx.recv() {
                            handle_batch(&app2, batch);
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!("watcher failed to start: {e}");
                    g.watcher_error = Some(e.to_string());
                }
            }
        }
        moved = run_waiting_if_automatic(&mut g);
        notify = g.cfg.settings.notifications;
    }
    if moved > 0 {
        if notify {
            send_notification(app, &format!("Organized {moved} file{}", plural(moved)));
        }
        let _ = app.emit("pending-changed", ());
        let _ = app.emit("history-changed", ());
    }
    emit_status(app);
}

/// Automatic mode also covers files that were already waiting (found at
/// startup, or queued before the setting was switched on), not just new
/// ones. Returns how many files were moved.
fn run_waiting_if_automatic(g: &mut Inner) -> usize {
    if g.cfg.settings.paused {
        return 0;
    }
    let mut moved = 0;
    if g.cfg.settings.auto_organize && !g.pending_super.is_empty() {
        g.pending_super.retain(|p| p.exists());
        let plan = sf_engine::plan_inputs(
            &g.pending_super,
            &g.cfg.rules,
            &g.cfg.settings,
            PlanMode::OrganizeRoot,
        );
        moved += run_plan(g, &plan).moved;
    }
    if !g.cfg.settings.confirm_before_move && !g.pending_watch.is_empty() {
        g.pending_watch.retain(|p| p.exists());
        let plan = sf_engine::plan_inputs(
            &g.pending_watch,
            &g.cfg.rules,
            &g.cfg.settings,
            PlanMode::WatchFolder,
        );
        moved += run_plan(g, &plan).moved;
    }
    moved
}

fn in_organize_root(path: &Path, settings: &Settings) -> bool {
    settings
        .organize_root
        .as_ref()
        .is_some_and(|sf| path.starts_with(sf))
}

/// New stable files from the watcher: organize right away or queue them,
/// depending on the settings.
fn handle_batch(app: &AppHandle, batch: Vec<PathBuf>) {
    let state = app.state::<AppState>();
    let mut moved = 0usize;
    let mut queued = 0usize;
    let notify;
    {
        let mut g = state.lock();
        if g.cfg.settings.paused {
            return;
        }
        notify = g.cfg.settings.notifications;
        let (sup, wat): (Vec<PathBuf>, Vec<PathBuf>) = batch
            .into_iter()
            .partition(|p| in_organize_root(p, &g.cfg.settings));

        for (paths, mode) in [(sup, PlanMode::OrganizeRoot), (wat, PlanMode::WatchFolder)] {
            if paths.is_empty() {
                continue;
            }
            let plan = sf_engine::plan_inputs(&paths, &g.cfg.rules, &g.cfg.settings, mode);
            let auto = match mode {
                PlanMode::OrganizeRoot => g.cfg.settings.auto_organize,
                PlanMode::WatchFolder => !g.cfg.settings.confirm_before_move,
            };
            if auto {
                moved += run_plan(&mut g, &plan).moved;
            } else {
                for op in plan.iter().filter(|op| op.is_executable()) {
                    let list = match mode {
                        PlanMode::OrganizeRoot => &mut g.pending_super,
                        PlanMode::WatchFolder => &mut g.pending_watch,
                    };
                    if !list.contains(&op.src) {
                        list.push(op.src.clone());
                        queued += 1;
                    }
                }
                note_skips(&mut g, &plan);
            }
        }
    }
    if notify {
        if moved > 0 {
            send_notification(app, &format!("Organized {moved} file{}", plural(moved)));
        }
        if queued > 0 {
            send_notification(
                app,
                &format!(
                    "{queued} new file{} waiting to be organized",
                    plural(queued)
                ),
            );
        }
    }
    let _ = app.emit("pending-changed", ());
    let _ = app.emit("history-changed", ());
    emit_status(app);
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

fn send_notification(app: &AppHandle, body: &str) {
    if let Err(e) = app
        .notification()
        .builder()
        .title("DeskZero")
        .body(body)
        .show()
    {
        tracing::warn!("notification failed: {e}");
    }
}

/// Skipped files (no rule, conflict under Skip) need the user's attention.
fn note_skips(g: &mut Inner, plan: &[PlannedOp]) {
    for op in plan {
        if let PlanStatus::Skip { reason } = &op.status {
            g.add_attention(op.src.clone(), reason.clone());
        }
    }
}

/// Execute a plan and update pending/attention bookkeeping.
pub fn run_plan(g: &mut Inner, plan: &[PlannedOp]) -> RunSummary {
    let history = g.cfg.history_path();
    let quarantine = g.cfg.data_dir.join("quarantine");
    let mut counter = g.counter;
    let result = mover::execute(
        plan,
        &history,
        &quarantine,
        &mut counter,
        AskResolution::Skip,
    );
    g.counter = counter;

    let done: Vec<PathBuf> = plan.iter().map(|op| op.src.clone()).collect();
    g.pending_super.retain(|p| !done.contains(p));
    g.pending_watch.retain(|p| !done.contains(p));
    note_skips(g, plan);

    match result {
        Ok(r) => {
            for item in &r.entry.items {
                g.attention.retain(|a| a.path != item.src);
            }
            let failed: Vec<FailedDto> = r
                .entry
                .failed
                .iter()
                .map(|f| FailedDto {
                    path: f.src.clone(),
                    error: f.error.clone(),
                })
                .collect();
            for f in &failed {
                g.add_attention(f.path.clone(), f.error.clone());
            }
            RunSummary {
                moved: r.entry.items.len(),
                failed,
                skipped: r.skipped.len(),
            }
        }
        Err(e) => {
            // Moves may have happened but history could not be written.
            tracing::error!("execute failed: {e}");
            RunSummary {
                moved: 0,
                failed: vec![FailedDto {
                    path: history,
                    error: format!("could not record history: {e}"),
                }],
                skipped: 0,
            }
        }
    }
}

/// Plan everything that is waiting, without moving anything.
pub fn pending_plan(g: &mut Inner) -> Vec<PlannedOp> {
    g.pending_super.retain(|p| p.exists());
    g.pending_watch.retain(|p| p.exists());
    let mut plan = sf_engine::plan_inputs(
        &g.pending_super,
        &g.cfg.rules,
        &g.cfg.settings,
        PlanMode::OrganizeRoot,
    );
    plan.extend(sf_engine::plan_inputs(
        &g.pending_watch,
        &g.cfg.rules,
        &g.cfg.settings,
        PlanMode::WatchFolder,
    ));
    plan
}

/// Files sitting loose at the top of your DeskZero (dropped while the
/// app was closed, or before a watcher existed) count as pending too.
pub fn scan_organize_root(g: &mut Inner) {
    let Some(sf) = g.cfg.settings.organize_root.clone() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&sf) else {
        return;
    };
    let loose: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    let plan = sf_engine::plan_inputs(
        &loose,
        &g.cfg.rules,
        &g.cfg.settings,
        PlanMode::OrganizeRoot,
    );
    for op in plan.iter().filter(|op| op.is_executable()) {
        if !g.pending_super.contains(&op.src) {
            g.pending_super.push(op.src.clone());
        }
    }
}

pub fn status(g: &Inner) -> Status {
    let today = chrono::Local::now().date_naive();
    let organized_today = sf_engine::history::read(&g.cfg.history_path())
        .iter()
        .filter(|e| e.kind == HistoryKind::Move)
        .filter(|e| e.ts.with_timezone(&chrono::Local).date_naive() == today)
        .map(|e| e.items.len())
        .sum();
    Status {
        organized_today,
        needs_attention: g.attention.len(),
        pending: g.pending_super.len() + g.pending_watch.len(),
        paused: g.cfg.settings.paused,
        watching: if g.watcher.is_some() {
            watched_folders(&g.cfg.settings)
        } else {
            Vec::new()
        },
        organize_root: g.cfg.settings.organize_root.clone(),
        watcher_error: g.watcher_error.clone(),
    }
}

pub fn emit_status(app: &AppHandle) {
    let s = status(&app.state::<AppState>().lock());
    tray::refresh(app, &s);
    let _ = app.emit("status", &s);
}
