//! Tauri commands: thin adapters over sf-engine. Anything that walks the
//! disk runs on a blocking thread so the window never freezes.

use crate::organizer::{self, RunSummary, Status};
use crate::state::{AppState, Attention};
use serde::Serialize;
use sf_engine::types::{PlanMode, PlannedOp, Rule, Settings};
use sf_engine::{HistoryEntry, HistoryKind};
use std::collections::HashSet;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

type CmdResult<T> = Result<T, String>;

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("background task failed: {e}"))?
}

// ---------- config ----------

#[derive(Serialize)]
pub struct ConfigDto {
    settings: Settings,
    rules: Vec<Rule>,
    data_dir: PathBuf,
    suggested_super_folder: Option<PathBuf>,
}

#[tauri::command]
pub fn get_config(app: AppHandle, state: State<'_, AppState>) -> ConfigDto {
    let g = state.lock();
    ConfigDto {
        settings: g.cfg.settings.clone(),
        rules: g.cfg.rules.clone(),
        data_dir: g.cfg.data_dir.clone(),
        suggested_super_folder: app.path().home_dir().ok().map(|h| h.join("Super Folder")),
    }
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> CmdResult<()> {
    validate_settings(&settings)?;
    if let Some(sf) = &settings.super_folder {
        std::fs::create_dir_all(sf).map_err(|e| format!("cannot create {}: {e}", sf.display()))?;
    }
    {
        let mut g = state.lock();
        g.cfg.settings = settings;
        g.cfg
            .save_settings()
            .map_err(|e| format!("could not save settings: {e}"))?;
        organizer::scan_super_folder(&mut g);
    }
    organizer::restart_watcher(&app);
    let _ = app.emit("pending-changed", ());
    Ok(())
}

/// Guards from the spec: absolute paths only, and the Super Folder and
/// watch folders must not contain one another.
fn validate_settings(s: &Settings) -> CmdResult<()> {
    let mut all: Vec<&PathBuf> = s.watch_folders.iter().map(|w| &w.path).collect();
    if let Some(sf) = &s.super_folder {
        all.push(sf);
    }
    for p in &all {
        if !p.is_absolute() {
            return Err(format!("{} is not a full path", p.display()));
        }
    }
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            if a.starts_with(b) || b.starts_with(a) {
                return Err(format!(
                    "{} and {} overlap; folders can't be inside each other",
                    a.display(),
                    b.display()
                ));
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn save_rules(app: AppHandle, state: State<'_, AppState>, rules: Vec<Rule>) -> CmdResult<()> {
    let mut seen = HashSet::new();
    for r in &rules {
        if r.name.trim().is_empty() {
            return Err("every rule needs a name".into());
        }
        if r.conditions.is_empty() {
            return Err(format!("rule \"{}\" has no conditions", r.name));
        }
        if !seen.insert(r.id.clone()) {
            return Err(format!("duplicate rule id {}", r.id));
        }
    }
    let mut g = state.lock();
    g.cfg.rules = rules;
    g.cfg
        .save_rules()
        .map_err(|e| format!("could not save rules: {e}"))?;
    drop(g);
    let _ = app.emit("pending-changed", ());
    Ok(())
}

// ---------- organize ----------

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> Status {
    organizer::status(&state.lock())
}

/// Plan dropped paths (from anywhere) into the Super Folder. No moves.
#[tauri::command]
pub fn plan_paths(state: State<'_, AppState>, paths: Vec<PathBuf>) -> CmdResult<Vec<PlannedOp>> {
    let g = state.lock();
    if g.cfg.settings.super_folder.is_none() {
        return Err("choose a Super Folder in Settings first".into());
    }
    Ok(sf_engine::plan_inputs(
        &paths,
        &g.cfg.rules,
        &g.cfg.settings,
        PlanMode::SuperFolder,
    ))
}

#[tauri::command]
pub fn pending_plan(state: State<'_, AppState>) -> Vec<PlannedOp> {
    organizer::pending_plan(&mut state.lock())
}

/// Execute exactly the plan the user previewed.
#[tauri::command]
pub async fn execute_plan(app: AppHandle, plan: Vec<PlannedOp>) -> CmdResult<RunSummary> {
    let app2 = app.clone();
    let summary = blocking(move || {
        let state = app2.state::<AppState>();
        let mut g = state.lock();
        Ok(organizer::run_plan(&mut g, &plan))
    })
    .await?;
    let _ = app.emit("pending-changed", ());
    let _ = app.emit("history-changed", ());
    organizer::emit_status(&app);
    Ok(summary)
}

#[tauri::command]
pub fn set_paused(app: AppHandle, state: State<'_, AppState>, paused: bool) -> CmdResult<()> {
    {
        let mut g = state.lock();
        g.cfg.settings.paused = paused;
        g.cfg
            .save_settings()
            .map_err(|e| format!("could not save settings: {e}"))?;
    }
    organizer::restart_watcher(&app);
    Ok(())
}

#[tauri::command]
pub fn get_attention(state: State<'_, AppState>) -> Vec<Attention> {
    state.lock().attention.clone()
}

#[tauri::command]
pub fn clear_attention(app: AppHandle, state: State<'_, AppState>) {
    state.lock().attention.clear();
    organizer::emit_status(&app);
}

// ---------- history ----------

#[derive(Serialize)]
pub struct HistoryDto {
    #[serde(flatten)]
    entry: HistoryEntry,
    /// A move batch whose items were all moved back by a later undo.
    undone: bool,
}

#[tauri::command]
pub fn get_history(state: State<'_, AppState>) -> Vec<HistoryDto> {
    let entries = sf_engine::history::read(&state.lock().cfg.history_path());
    // Undo entries record items reversed (dst → src).
    let undone_pairs: HashSet<(PathBuf, PathBuf)> = entries
        .iter()
        .filter(|e| e.kind == HistoryKind::Undo)
        .flat_map(|e| e.items.iter().map(|i| (i.dst.clone(), i.src.clone())))
        .collect();
    entries
        .into_iter()
        .map(|e| {
            let undone = e.kind == HistoryKind::Move
                && !e.items.is_empty()
                && e.items
                    .iter()
                    .all(|i| undone_pairs.contains(&(i.src.clone(), i.dst.clone())));
            HistoryDto { entry: e, undone }
        })
        .collect()
}

#[tauri::command]
pub async fn undo_batch(app: AppHandle, id: String) -> CmdResult<RunSummary> {
    let app2 = app.clone();
    let summary = blocking(move || {
        let state = app2.state::<AppState>();
        let mut g = state.lock();
        let hist = g.cfg.history_path();
        let entry = sf_engine::history::read(&hist)
            .into_iter()
            .find(|e| e.id == id && e.kind == HistoryKind::Move)
            .ok_or("that batch is no longer in the history")?;
        let mut counter = g.counter;
        let undo =
            sf_engine::mover::undo(&entry, &hist, &mut counter).map_err(|e| e.to_string())?;
        g.counter = counter;
        Ok(RunSummary {
            moved: undo.items.len(),
            failed: undo
                .failed
                .iter()
                .map(|f| organizer::FailedDto {
                    path: f.src.clone(),
                    error: f.error.clone(),
                })
                .collect(),
            skipped: 0,
        })
    })
    .await?;
    let _ = app.emit("history-changed", ());
    organizer::emit_status(&app);
    Ok(summary)
}

// ---------- tools: duplicates, search, rename ----------

#[derive(Serialize)]
pub struct DupGroupDto {
    size_bytes: u64,
    sha256: String,
    files: Vec<FileDto>,
}

#[derive(Serialize)]
pub struct FileDto {
    path: PathBuf,
    size_bytes: u64,
    modified: Option<String>,
    category: Option<sf_engine::Category>,
}

fn time_str(t: Option<std::time::SystemTime>) -> Option<String> {
    t.map(|t| chrono::DateTime::<chrono::Local>::from(t).to_rfc3339())
}

#[tauri::command]
pub async fn find_duplicates(folder: PathBuf) -> CmdResult<Vec<DupGroupDto>> {
    blocking(move || {
        let groups = sf_engine::duplicates::find_duplicates(&folder).map_err(|e| e.to_string())?;
        Ok(groups
            .into_iter()
            .map(|g| DupGroupDto {
                size_bytes: g.size_bytes,
                sha256: g.sha256,
                files: g
                    .files
                    .into_iter()
                    .map(|f| FileDto {
                        path: f.path,
                        size_bytes: g.size_bytes,
                        modified: time_str(f.modified),
                        category: None,
                    })
                    .collect(),
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn search_files(folder: PathBuf, query: String) -> CmdResult<Vec<FileDto>> {
    blocking(move || {
        let hits = sf_engine::search::search(&folder, &query).map_err(|e| e.to_string())?;
        Ok(hits
            .into_iter()
            .take(2000)
            .map(|h| FileDto {
                path: h.path,
                size_bytes: h.size_bytes,
                modified: time_str(h.modified),
                category: Some(h.category),
            })
            .collect())
    })
    .await
}

#[derive(Serialize)]
pub struct RenameDto {
    src: PathBuf,
    dst: PathBuf,
    skipped: bool,
    reason: Option<String>,
}

/// Top-level files of a folder, sorted: the same input the CLI uses so
/// {counter} numbering is predictable.
fn folder_files(folder: &PathBuf) -> CmdResult<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(folder)
        .map_err(|e| format!("cannot read {}: {e}", folder.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    paths.sort();
    Ok(paths)
}

#[tauri::command]
pub async fn rename_preview(folder: PathBuf, template: String) -> CmdResult<Vec<RenameDto>> {
    blocking(move || {
        let ops = sf_engine::renamer::plan_rename(&folder_files(&folder)?, &template)
            .map_err(|e| e.to_string())?;
        Ok(ops
            .into_iter()
            .map(|o| RenameDto {
                src: o.src,
                dst: o.dst,
                skipped: o.skipped,
                reason: o.reason,
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn rename_apply(folder: PathBuf, template: String) -> CmdResult<RunSummary> {
    blocking(move || {
        let ops = sf_engine::renamer::plan_rename(&folder_files(&folder)?, &template)
            .map_err(|e| e.to_string())?;
        let (n, failed) = sf_engine::renamer::apply_rename(&ops);
        Ok(RunSummary {
            moved: n,
            failed: failed
                .into_iter()
                .map(|(path, error)| organizer::FailedDto { path, error })
                .collect(),
            skipped: ops.iter().filter(|o| o.skipped).count(),
        })
    })
    .await
}

// ---------- shell ----------

#[tauri::command]
pub fn open_folder(app: AppHandle, path: PathBuf) -> CmdResult<()> {
    if !path.is_dir() {
        return Err(format!("{} is not a folder", path.display()));
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reveal_file(app: AppHandle, path: PathBuf) -> CmdResult<()> {
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| e.to_string())
}
