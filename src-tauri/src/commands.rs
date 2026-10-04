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
use tauri_plugin_autostart::ManagerExt;
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
    suggested_organize_root: Option<PathBuf>,
}

#[tauri::command]
pub fn get_config(app: AppHandle, state: State<'_, AppState>) -> ConfigDto {
    let g = state.lock();
    ConfigDto {
        settings: g.cfg.settings.clone(),
        rules: g.cfg.rules.clone(),
        data_dir: g.cfg.data_dir.clone(),
        suggested_organize_root: app.path().home_dir().ok().map(|h| h.join("DeskZero")),
    }
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> CmdResult<()> {
    validate_settings(&settings)?;
    if let Some(sf) = &settings.organize_root {
        std::fs::create_dir_all(sf).map_err(|e| format!("cannot create {}: {e}", sf.display()))?;
    }
    apply_autostart(&app, settings.start_with_system)?;
    let (menu_was, roots_were) = {
        let g = state.lock();
        (
            g.cfg.settings.context_menu,
            g.cfg.settings.search_roots.clone(),
        )
    };
    if menu_was != settings.context_menu {
        crate::shell::set_context_menu(settings.context_menu)?;
    }
    let roots_changed = roots_were != settings.search_roots;
    {
        let mut g = state.lock();
        g.cfg.settings = settings;
        g.cfg
            .save_settings()
            .map_err(|e| format!("could not save settings: {e}"))?;
        organizer::scan_organize_root(&mut g);
    }
    organizer::restart_watcher(&app);
    if roots_changed {
        crate::finder::rebuild(&app);
    }
    let _ = app.emit("pending-changed", ());
    Ok(())
}

/// Register or remove the login item to match the setting.
fn apply_autostart(app: &AppHandle, wanted: bool) -> CmdResult<()> {
    let launcher = app.autolaunch();
    let current = launcher.is_enabled().unwrap_or(false);
    if wanted == current {
        return Ok(());
    }
    let result = if wanted {
        launcher.enable()
    } else {
        launcher.disable()
    };
    result.map_err(|e| format!("could not change start with system: {e}"))
}

/// Guards from the spec: absolute paths only, and your DeskZero and
/// watch folders must not contain one another.
fn validate_settings(s: &Settings) -> CmdResult<()> {
    let mut all: Vec<&PathBuf> = s.watch_folders.iter().map(|w| &w.path).collect();
    if let Some(sf) = &s.organize_root {
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

/// A rule must have a name and conditions, and a relative destination must
/// stay inside your DeskZero (no `..`, no drive or root prefix).
fn validate_rule(r: &Rule) -> CmdResult<()> {
    if r.name.trim().is_empty() {
        return Err("every rule needs a name".into());
    }
    if r.conditions.is_empty() {
        return Err(format!("rule \"{}\" has no conditions", r.name));
    }
    if let sf_engine::types::Destination::Custom { path } = &r.destination {
        if path.as_os_str().is_empty() {
            return Err(format!("rule \"{}\" has no destination folder", r.name));
        }
        if !path.is_absolute() {
            let escapes = path.components().any(|c| {
                !matches!(
                    c,
                    std::path::Component::Normal(_) | std::path::Component::CurDir
                )
            });
            if escapes {
                return Err(format!(
                    "rule \"{}\": a relative destination must stay inside your DeskZero",
                    r.name
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
        validate_rule(r)?;
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

/// Plan dropped paths (from anywhere) into your DeskZero. No moves.
#[tauri::command]
pub fn plan_paths(state: State<'_, AppState>, paths: Vec<PathBuf>) -> CmdResult<Vec<PlannedOp>> {
    let g = state.lock();
    if g.cfg.settings.organize_root.is_none() {
        return Err("choose a DeskZero in Settings first".into());
    }
    Ok(sf_engine::plan_inputs(
        &paths,
        &g.cfg.rules,
        &g.cfg.settings,
        PlanMode::OrganizeRoot,
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
        let protect = protected_folders(&g.cfg.settings);
        let undo = sf_engine::mover::undo_with_cleanup(&entry, &hist, &mut counter, &protect)
            .map_err(|e| e.to_string())?;
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
pub async fn rename_apply(
    app: AppHandle,
    folder: PathBuf,
    template: String,
) -> CmdResult<RunSummary> {
    let app2 = app.clone();
    let summary = blocking(move || {
        let ops = sf_engine::renamer::plan_rename(&folder_files(&folder)?, &template)
            .map_err(|e| e.to_string())?;
        let (n, failed) = sf_engine::renamer::apply_rename(&ops);

        // Record the renames as a normal batch so History can undo them.
        let failed_srcs: HashSet<&PathBuf> = failed.iter().map(|(p, _)| p).collect();
        let items: Vec<sf_engine::history::HistoryItem> = ops
            .iter()
            .filter(|o| !o.skipped && o.src != o.dst && !failed_srcs.contains(&o.src))
            .map(|o| sf_engine::history::HistoryItem {
                src: o.src.clone(),
                dst: o.dst.clone(),
            })
            .collect();
        if !items.is_empty() {
            let state = app2.state::<AppState>();
            let mut g = state.lock();
            g.counter += 1;
            let entry = HistoryEntry {
                id: sf_engine::history::new_op_id(g.counter),
                ts: chrono::Utc::now(),
                kind: HistoryKind::Move,
                items,
                quarantined: Vec::new(),
                failed: Vec::new(),
            };
            sf_engine::history::append(&g.cfg.history_path(), &entry)
                .map_err(|e| format!("renamed, but could not record history: {e}"))?;
        }
        Ok(RunSummary {
            moved: n,
            failed: failed
                .into_iter()
                .map(|(path, error)| organizer::FailedDto { path, error })
                .collect(),
            skipped: ops.iter().filter(|o| o.skipped).count(),
        })
    })
    .await?;
    let _ = app.emit("history-changed", ());
    Ok(summary)
}

// ---------- rules import / export ----------

#[derive(Serialize, serde::Deserialize)]
struct RulesExport {
    #[serde(default = "rules_export_version")]
    version: u32,
    rules: Vec<Rule>,
}

fn rules_export_version() -> u32 {
    1
}

#[tauri::command]
pub fn export_rules(state: State<'_, AppState>, path: PathBuf) -> CmdResult<usize> {
    let rules = state.lock().cfg.rules.clone();
    let n = rules.len();
    sf_engine::config::write_atomic(&path, &RulesExport { version: 1, rules })
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(n)
}

/// Import a rules.json. `replace` swaps the whole list; otherwise the
/// imported rules are appended (ids that already exist get a new one).
#[tauri::command]
pub fn import_rules(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
    replace: bool,
) -> CmdResult<usize> {
    let meta =
        std::fs::metadata(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if meta.len() > 2_000_000 {
        return Err("that file is too big to be a rules file".into());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let parsed: RulesExport =
        serde_json::from_str(&text).map_err(|e| format!("not a DeskZero rules file: {e}"))?;
    let mut incoming = parsed.rules;
    for r in &incoming {
        validate_rule(r)?;
    }
    let n = incoming.len();
    let mut g = state.lock();
    let mut rules = if replace {
        Vec::new()
    } else {
        g.cfg.rules.clone()
    };
    let mut ids: HashSet<String> = rules.iter().map(|r| r.id.clone()).collect();
    let stamp = chrono::Utc::now().timestamp_millis();
    for (i, r) in incoming.iter_mut().enumerate() {
        if !ids.insert(r.id.clone()) {
            r.id = format!("rule_{stamp}_{i}");
            ids.insert(r.id.clone());
        }
    }
    rules.extend(incoming);
    g.cfg.rules = rules;
    g.cfg
        .save_rules()
        .map_err(|e| format!("could not save rules: {e}"))?;
    drop(g);
    let _ = app.emit("pending-changed", ());
    Ok(n)
}

/// Folders undo must never remove even when empty.
fn protected_folders(s: &Settings) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = s.organize_root.iter().cloned().collect();
    v.extend(s.watch_folders.iter().map(|w| w.path.clone()));
    v
}

// ---------- tidy: conflicts, duplicates, cleanup, storage ----------

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    KeepBoth,
    Replace,
    Skip,
}

/// Apply the user's choices for "a file with this name exists" (Ask
/// policy). Keep both picks a free "name (1)" that no other op claims.
#[tauri::command]
pub fn resolve_decisions(
    mut plan: Vec<PlannedOp>,
    decisions: Vec<(PathBuf, Decision)>,
) -> CmdResult<Vec<PlannedOp>> {
    use sf_engine::types::PlanStatus;
    let mut claimed: Vec<PathBuf> = plan
        .iter()
        .filter(|o| matches!(o.status, PlanStatus::Move { .. }))
        .map(|o| o.dst.clone())
        .collect();
    for op in plan.iter_mut() {
        if op.status != PlanStatus::NeedsDecision {
            continue;
        }
        let choice = decisions
            .iter()
            .find(|(src, _)| *src == op.src)
            .map(|(_, d)| d);
        match choice {
            Some(Decision::Replace) => op.status = PlanStatus::Move { replace: true },
            Some(Decision::Skip) | None => {
                op.status = PlanStatus::Skip {
                    reason: "skipped by you".into(),
                }
            }
            Some(Decision::KeepBoth) => {
                let free =
                    sf_engine::fsutil::free_target(&op.dst, &claimed).map_err(|e| e.to_string())?;
                claimed.push(free.clone());
                op.dst = free;
                op.status = PlanStatus::Move { replace: false };
            }
        }
    }
    Ok(plan)
}

#[tauri::command]
pub async fn plan_move_into(files: Vec<PathBuf>, dest: PathBuf) -> CmdResult<Vec<PlannedOp>> {
    blocking(move || Ok(sf_engine::tidy::plan_move_into(&files, &dest))).await
}

#[tauri::command]
pub async fn plan_old_files(
    folder: PathBuf,
    days: u32,
    dest: PathBuf,
) -> CmdResult<Vec<PlannedOp>> {
    blocking(move || {
        sf_engine::tidy::plan_old_files(&folder, days, &dest)
            .map_err(|e| format!("cannot read {}: {e}", folder.display()))
    })
    .await
}

#[tauri::command]
pub async fn storage_overview(app: AppHandle) -> CmdResult<Vec<sf_engine::tidy::FolderUsage>> {
    let root = app
        .state::<AppState>()
        .lock()
        .cfg
        .settings
        .organize_root
        .clone();
    let Some(root) = root else {
        return Ok(Vec::new());
    };
    blocking(move || sf_engine::tidy::storage_overview(&root).map_err(|e| e.to_string())).await
}

// ---------- universal search ----------

#[tauri::command]
pub fn universal_search(
    finder: State<'_, crate::finder::Finder>,
    query: String,
) -> Vec<sf_engine::search_index::Entry> {
    finder.query(&query, 12)
}

#[tauri::command]
pub fn search_info(app: AppHandle) -> crate::finder::FinderInfo {
    crate::finder::info(&app)
}

#[tauri::command]
pub fn rebuild_search(app: AppHandle) {
    crate::finder::rebuild(&app);
}

#[tauri::command]
pub fn launch(app: AppHandle, path: PathBuf) -> CmdResult<()> {
    if !path.exists() {
        return Err(format!("{} no longer exists", path.display()));
    }
    crate::shell::launch(&app, &path)
}

// ---------- shell ----------

#[tauri::command]
pub fn platform() -> &'static str {
    std::env::consts::OS
}

/// Paths passed with `--organize` before the window was ready.
#[tauri::command]
pub fn take_startup_paths(state: State<'_, AppState>) -> Vec<PathBuf> {
    std::mem::take(&mut state.lock().startup_paths)
}

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

/// Open one of a few fixed web pages (developer profile, sister tools).
/// Ids only, so the page can't be made to open arbitrary URLs.
#[tauri::command]
pub fn open_link(app: AppHandle, id: String) -> CmdResult<()> {
    let url = match id.as_str() {
        "developer" => "https://github.com/imjagdeep",
        "deskzero" => "https://github.com/imjagdeep/deskzero",
        "deskmedic" => "https://github.com/imjagdeep/DeskMedic",
        _ => return Err(format!("unknown link {id}")),
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}
