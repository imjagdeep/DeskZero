//! Desktop integration: the Explorer right-click entry (Windows), files
//! passed on the command line (`--organize <path>`), and opening things.

use std::path::{Path, PathBuf};

/// Paths after `--organize` in a command line.
pub fn organize_args(args: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut take = false;
    for a in args {
        if a == "--organize" {
            take = true;
        } else if take && !a.starts_with("--") {
            out.push(PathBuf::from(a));
        }
    }
    out
}

/// Add or remove "Organize with Super Folder" in the right-click menu for
/// files and folders. Current user only, no admin rights needed.
#[cfg(windows)]
pub fn set_context_menu(enabled: bool) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let keys = [
        r"Software\Classes\*\shell\SuperFolder",
        r"Software\Classes\Directory\shell\SuperFolder",
    ];
    if !enabled {
        for k in keys {
            // Missing key = already removed.
            let _ = hkcu.delete_subkey_all(k);
        }
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| format!("cannot find the app: {e}"))?;
    let exe = exe.to_string_lossy();
    for k in keys {
        let (key, _) = hkcu
            .create_subkey(k)
            .map_err(|e| format!("cannot write the right-click menu: {e}"))?;
        key.set_value("", &"Organize with Super Folder")
            .and_then(|_| key.set_value("Icon", &format!("\"{exe}\",0")))
            .map_err(|e| format!("cannot write the right-click menu: {e}"))?;
        let (cmd, _) = key
            .create_subkey("command")
            .map_err(|e| format!("cannot write the right-click menu: {e}"))?;
        cmd.set_value("", &format!("\"{exe}\" --organize \"%1\""))
            .map_err(|e| format!("cannot write the right-click menu: {e}"))?;
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set_context_menu(enabled: bool) -> Result<(), String> {
    if enabled {
        Err("the right-click menu is only available on Windows for now".into())
    } else {
        Ok(())
    }
}

/// Launch a search result. Linux `.desktop` entries are started through
/// `gtk-launch` (opening them would show the file in an editor).
pub fn launch(app: &tauri::AppHandle, path: &Path) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if cfg!(target_os = "linux") && path.extension().is_some_and(|e| e == "desktop") {
        let id = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        return std::process::Command::new("gtk-launch")
            .arg(id)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("could not start the app: {e}"));
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn organize_args_collects_paths() {
        let args: Vec<String> = ["app.exe", "--organize", "C:\\a b\\x.pdf", "y.txt"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            organize_args(&args),
            vec![PathBuf::from("C:\\a b\\x.pdf"), PathBuf::from("y.txt")]
        );
        assert!(organize_args(&["app.exe".to_string()]).is_empty());
    }
}
