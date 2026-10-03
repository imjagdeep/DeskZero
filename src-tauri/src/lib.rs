//! Super Folder desktop shell: window, tray and commands over sf-engine.

mod commands;
mod organizer;
mod state;
mod tray;

use state::AppState;
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
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
            app.manage(state);

            tray::build(app.handle())?;
            organizer::restart_watcher(app.handle());

            if !start_minimized {
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
            commands::open_folder,
            commands::reveal_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Super Folder");
}
