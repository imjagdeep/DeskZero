//! Super Folder desktop shell: window, tray and commands over sf-engine.

mod commands;
mod finder;
mod organizer;
mod shell;
mod state;
mod tray;

use state::AppState;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Cmd on macOS, Ctrl elsewhere.
#[cfg(target_os = "macos")]
const PRIMARY: Modifiers = Modifiers::SUPER;
#[cfg(not(target_os = "macos"))]
const PRIMARY: Modifiers = Modifiers::CONTROL;

/// Ctrl/Cmd + Shift + Space: universal search.
fn search_shortcut() -> Shortcut {
    Shortcut::new(Some(PRIMARY | Modifiers::SHIFT), Code::Space)
}

/// Ctrl/Cmd + Alt + O: organize everything waiting.
fn organize_shortcut() -> Shortcut {
    Shortcut::new(Some(PRIMARY | Modifiers::ALT), Code::KeyO)
}

/// Files handed over by the right-click menu (or a second launch).
fn organize_from_args(app: &AppHandle, args: &[String]) {
    let paths = shell::organize_args(args);
    tray::show_main(app);
    if !paths.is_empty() {
        let _ = app.emit("organize-paths", paths);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first: a second launch just focuses the running window.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            organize_from_args(app, &args);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if shortcut == &search_shortcut() {
                        finder::toggle(app);
                    } else if shortcut == &organize_shortcut() {
                        tray::show_main(app);
                        let _ = app.emit("organize-now", ());
                    }
                })
                .build(),
        )
        .setup(|app| {
            // Same data dir as the CLI harness, so both see one history.
            // SUPER_FOLDER_DATA_DIR points it elsewhere (testing, portable use).
            let data_dir = std::env::var_os("SUPER_FOLDER_DATA_DIR")
                .map(std::path::PathBuf::from)
                .or_else(sf_engine::Config::default_dir)
                .ok_or("no per-user config folder")?;
            std::fs::create_dir_all(&data_dir)?;
            let mut cfg = sf_engine::Config::load(&data_dir);
            if let Err(e) =
                sf_engine::history::compact(&cfg.history_path(), cfg.settings.keep_history_days)
            {
                tracing::warn!("history compaction failed: {e}");
            }
            if let Some(sf) = &cfg.settings.super_folder {
                if let Err(e) = std::fs::create_dir_all(sf) {
                    tracing::warn!("cannot create super folder {}: {e}", sf.display());
                }
            }
            cfg.settings.version = cfg.settings.version.max(1);
            let start_minimized = cfg.settings.start_minimized;

            let state = AppState::new(cfg);
            organizer::scan_super_folder(&mut state.lock());
            // Launched from the right-click menu: keep the paths until the
            // window asks for them.
            let args: Vec<String> = std::env::args().collect();
            state.lock().startup_paths = shell::organize_args(&args);
            app.manage(state);
            app.manage(finder::Finder::default());
            finder::start(app.handle());

            for sc in [search_shortcut(), organize_shortcut()] {
                if let Err(e) = app.global_shortcut().register(sc) {
                    // Another app owns the key: everything else still works.
                    tracing::warn!("could not register shortcut {sc:?}: {e}");
                }
            }

            tray::build(app.handle())?;
            organizer::restart_watcher(app.handle());

            let has_paths = !app.state::<AppState>().lock().startup_paths.is_empty();
            if !start_minimized || has_paths {
                tray::show_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps the organizer running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_settings,
            commands::save_rules,
            commands::get_status,
            commands::plan_paths,
            commands::pending_plan,
            commands::execute_plan,
            commands::set_paused,
            commands::get_attention,
            commands::clear_attention,
            commands::get_history,
            commands::undo_batch,
            commands::find_duplicates,
            commands::search_files,
            commands::rename_preview,
            commands::rename_apply,
            commands::export_rules,
            commands::import_rules,
            commands::open_folder,
            commands::reveal_file,
            commands::resolve_decisions,
            commands::plan_move_into,
            commands::plan_old_files,
            commands::storage_overview,
            commands::universal_search,
            commands::search_info,
            commands::rebuild_search,
            commands::launch,
            commands::platform,
            commands::take_startup_paths,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Super Folder");
}
