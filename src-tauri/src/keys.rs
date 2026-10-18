//! The two global shortcuts (universal search, organize now), chosen by the
//! user in Settings. Stored as text like "Control+Shift+Space" (the global
//! shortcut plugin's format); None means the default, "" means off.

use std::str::FromStr;
use std::sync::Mutex;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

pub const DEFAULT_SEARCH: &str = "CommandOrControl+Shift+Space";
pub const DEFAULT_ORGANIZE: &str = "CommandOrControl+Alt+KeyO";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Search,
    Organize,
}

/// What is registered right now.
#[derive(Default)]
pub struct Keys {
    search: Mutex<Option<Shortcut>>,
    organize: Mutex<Option<Shortcut>>,
}

impl Keys {
    fn slot(&self, a: Action) -> &Mutex<Option<Shortcut>> {
        match a {
            Action::Search => &self.search,
            Action::Organize => &self.organize,
        }
    }

    /// Which action a pressed shortcut belongs to.
    pub fn action_of(&self, pressed: &Shortcut) -> Option<Action> {
        [Action::Search, Action::Organize].into_iter().find(|a| {
            self.slot(*a)
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .as_ref()
                == Some(pressed)
        })
    }
}

/// The setting's text → a shortcut (None = off). Must have a modifier,
/// except the function keys, so plain typing is never taken.
pub fn parse(setting: Option<&str>, default: &str) -> Result<Option<Shortcut>, String> {
    let text = setting.unwrap_or(default).trim();
    if text.is_empty() {
        return Ok(None);
    }
    let sc = Shortcut::from_str(text)
        .map_err(|e| format!("\"{text}\" isn't a key combination DeskZero can use ({e})"))?;
    let key = text
        .rsplit('+')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_uppercase();
    let function_key =
        key.len() >= 2 && key.starts_with('F') && key[1..].chars().all(|c| c.is_ascii_digit());
    if !text.contains('+') && !function_key {
        return Err("add Ctrl, Alt, Shift or ⌘ so normal typing isn't taken".into());
    }
    Ok(Some(sc))
}

/// Register `wanted` for `action` in place of what it had. If the system or
/// another app already owns the keys, the old shortcut stays.
pub fn set(
    app: &AppHandle,
    keys: &Keys,
    action: Action,
    wanted: Option<Shortcut>,
) -> Result<(), String> {
    let gs = app.global_shortcut();
    let mut slot = keys.slot(action).lock().unwrap_or_else(|p| p.into_inner());
    if *slot == wanted {
        return Ok(());
    }
    if let Some(new) = wanted {
        let other = match action {
            Action::Search => Action::Organize,
            Action::Organize => Action::Search,
        };
        let taken_by_other =
            *keys.slot(other).lock().unwrap_or_else(|p| p.into_inner()) == Some(new);
        if taken_by_other {
            return Err("the other DeskZero shortcut already uses these keys".into());
        }
    }
    if let Some(old) = *slot {
        if let Err(e) = gs.unregister(old) {
            tracing::warn!("could not release {old:?}: {e}");
        }
    }
    if let Some(new) = wanted {
        if let Err(e) = gs.register(new) {
            // Put the old one back so the action still has a shortcut.
            if let Some(old) = *slot {
                if let Err(e2) = gs.register(old) {
                    tracing::warn!("could not restore {old:?}: {e2}");
                    *slot = None;
                }
            }
            return Err(format!(
                "these keys are already used by the system or another app ({e})"
            ));
        }
    }
    *slot = wanted;
    Ok(())
}

/// At start: register both from the settings; a bad or taken setting is
/// logged and that action simply has no shortcut.
pub fn register_from_settings(app: &AppHandle, keys: &Keys, s: &sf_engine::Settings) {
    for (action, setting, default) in [
        (Action::Search, s.search_shortcut.as_deref(), DEFAULT_SEARCH),
        (
            Action::Organize,
            s.organize_shortcut.as_deref(),
            DEFAULT_ORGANIZE,
        ),
    ] {
        let result = parse(setting, default).and_then(|sc| set(app, keys, action, sc));
        if let Err(e) = result {
            tracing::warn!("shortcut for {action:?}: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_defaults_off_and_custom() {
        assert!(parse(None, DEFAULT_SEARCH).unwrap().is_some());
        assert!(parse(None, DEFAULT_ORGANIZE).unwrap().is_some());
        assert!(parse(Some(""), DEFAULT_SEARCH).unwrap().is_none());
        assert!(parse(Some("Control+Alt+KeyK"), DEFAULT_SEARCH)
            .unwrap()
            .is_some());
        assert!(parse(Some("Super+Shift+Digit2"), DEFAULT_SEARCH)
            .unwrap()
            .is_some());
        assert!(parse(Some("F9"), DEFAULT_SEARCH).unwrap().is_some());
    }

    #[test]
    fn refuses_plain_keys_and_nonsense() {
        assert!(parse(Some("KeyA"), DEFAULT_SEARCH).is_err());
        assert!(parse(Some("Control+Banana"), DEFAULT_SEARCH).is_err());
        assert!(parse(Some("Control+"), DEFAULT_SEARCH).is_err());
    }
}
