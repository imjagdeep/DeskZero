//! System tray / menu bar: status line, Organize Now, Pause, Open, Exit.

use crate::organizer::Status;
use crate::state::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

/// Menu items whose text changes with the status.
struct TrayItems {
    status: MenuItem<Wry>,
    pause: MenuItem<Wry>,
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", "Not watching", false, None::<&str>)?;
    let organize = MenuItem::with_id(app, "organize", "Organize Now", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause", true, None::<&str>)?;
    let open_sf = MenuItem::with_id(app, "open_sf", "Open DeskZero", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Open", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &PredefinedMenuItem::separator(app)?,
            &organize,
            &pause,
            &PredefinedMenuItem::separator(app)?,
            &open_sf,
            &show,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("DeskZero")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "organize" => {
                show_main(app);
                let _ = app.emit("organize-now", ());
            }
            "pause" => {
                let paused = app.state::<AppState>().lock().cfg.settings.paused;
                if let Err(e) = crate::commands::set_paused(app.clone(), app.state(), !paused) {
                    tracing::warn!("pause toggle failed: {e}");
                }
            }
            "open_sf" => {
                let sf = app
                    .state::<AppState>()
                    .lock()
                    .cfg
                    .settings
                    .organize_root
                    .clone();
                match sf {
                    Some(p) => {
                        if let Err(e) = crate::commands::open_folder(app.clone(), p) {
                            tracing::warn!("open organize root failed: {e}");
                        }
                    }
                    None => {
                        show_main(app);
                        let _ = app.emit("navigate", "settings");
                    }
                }
            }
            "show" => show_main(app),
            "settings" => {
                show_main(app);
                let _ = app.emit("navigate", "settings");
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayItems { status, pause });
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn refresh(app: &AppHandle, s: &Status) {
    let Some(items) = app.try_state::<TrayItems>() else {
        return;
    };
    let text = if s.paused {
        "⏸ Monitoring paused".to_string()
    } else if s.watching.is_empty() {
        "Not watching any folder".to_string()
    } else {
        format!("● Watching {} folder(s)", s.watching.len())
    };
    let _ = items.status.set_text(text);
    let _ = items
        .pause
        .set_text(if s.paused { "Resume" } else { "Pause" });
    if let Some(tray) = app.tray_by_id("main") {
        let tip = format!(
            "DeskZero: {} organized today, {} waiting, {} need attention",
            s.organized_today, s.pending, s.needs_attention
        );
        let _ = tray.set_tooltip(Some(tip));
    }
}
