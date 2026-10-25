//! DeskZero desktop shell: window, tray and commands over sf-engine.

mod commands;
mod desklab;
mod drives;
mod finder;
mod icons;
mod keys;
mod organizer;
mod shell;
mod state;
mod tray;

use state::AppState;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

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
        .plugin(tauri_plugin_clipboard_manager::init())
        // Reopen where it was left (size, position, maximized), never hidden;
        // the search bar places itself.
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        - tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .with_denylist(&["spotlight"])
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    // Which one is the user's choice (keys.rs).
                    match app.state::<keys::Keys>().action_of(shortcut) {
                        Some(keys::Action::Search) => finder::toggle(app),
                        Some(keys::Action::Organize) => {
                            tray::show_main(app);
                            let _ = app.emit("organize-now", ());
                        }
                        None => {}
                    }
                })
                .build(),
        )
        .setup(|app| {
            // Same data dir as the CLI harness, so both see one history.
            // DESKZERO_DATA_DIR points it elsewhere (testing, portable use);
            // the old SUPER_FOLDER_DATA_DIR name still works.
            let data_dir = std::env::var_os("DESKZERO_DATA_DIR")
                .or_else(|| std::env::var_os("SUPER_FOLDER_DATA_DIR"))
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
            if let Some(sf) = &cfg.settings.organize_root {
                if let Err(e) = std::fs::create_dir_all(sf) {
                    tracing::warn!("cannot create organize root {}: {e}", sf.display());
                }
            }
            cfg.settings.version = cfg.settings.version.max(1);
            let start_minimized = cfg.settings.start_minimized;

            let state = AppState::new(cfg);
            organizer::scan_organize_root(&mut state.lock());
            // Launched from the right-click menu: keep the paths until the
            // window asks for them.
            let args: Vec<String> = std::env::args().collect();
            state.lock().startup_paths = shell::organize_args(&args);
            app.manage(state);
            app.manage(finder::Finder::default());
            app.manage(icons::IconCache::default());
            app.manage(commands::ThumbCache::default());
            app.manage(commands::FileClipboard::default());
            finder::start(app.handle());

            // Another app owning a key only costs that shortcut.
            app.manage(keys::Keys::default());
            {
                let settings = app.state::<AppState>().lock().cfg.settings.clone();
                keys::register_from_settings(app.handle(), &app.state::<keys::Keys>(), &settings);
            }

            tray::build(app.handle())?;
            organizer::restart_watcher(app.handle());

            // Catch up on files that reached a watch folder while DeskZero
            // was closed, then keep a heartbeat for the next start.
            if let Some(since) = sf_engine::alive::read(&data_dir) {
                organizer::catch_up(app.handle(), since);
            }
            let heartbeat_dir = data_dir.clone();
            std::thread::spawn(move || loop {
                if let Err(e) = sf_engine::alive::write(&heartbeat_dir) {
                    tracing::warn!("could not write the heartbeat: {e}");
                }
                std::thread::sleep(std::time::Duration::from_secs(60));
            });

            let has_paths = !app.state::<AppState>().lock().startup_paths.is_empty();
            // Started hidden (e.g. at login): menu bar / tray only.
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
            commands::search_icons,
            commands::search_info,
            commands::rebuild_search,
            commands::launch,
            commands::platform,
            commands::take_startup_paths,
            commands::open_link,
            commands::desklab_tools,
            commands::open_tool,
            commands::notify_update,
            commands::plan_watch_folder,
            commands::list_folder,
            commands::folder_sizes,
            commands::places,
            commands::rename_item,
            commands::new_folder,
            commands::copy_items,
            commands::move_folders,
            commands::trash_items,
            commands::thumbnails,
            commands::drives,
            commands::disk_space,
            commands::restore_from_trash,
            commands::remove_empty_folder,
            commands::cleanup_suggestions,
            commands::preview_image,
            commands::preview_text,
            commands::quick_look,
            commands::clipboard_set_files,
            commands::clipboard_get_files,
        ])
        .build(tauri::generate_context!())
        .expect("error while building DeskZero")
        .run(|_app, _event| {
            // macOS: opening DeskZero again (Finder, Spotlight, Launchpad)
            // while it only lives in the menu bar brings the window back.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                tray::show_main(_app);
            }
        });
}
