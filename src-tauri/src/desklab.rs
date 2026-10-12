//! The other DeskLab tools: whether one is installed, and starting it.
//! Same file in DeskZero, DeskMedic and DeskFolio.
//!
//! Only the installers' default places are checked (no searching the disk):
//! Windows `%LOCALAPPDATA%\<Name>\<id>.exe`, macOS `/Applications` or
//! `~/Applications`, Linux `/usr/bin/<id>` (the .deb). A portable copy or an
//! AppImage isn't found; the app then offers the download page instead.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

const TOOLS: [(&str, &str); 3] = [
    ("deskzero", "DeskZero"),
    ("deskmedic", "DeskMedic"),
    ("deskfolio", "DeskFolio"),
];

#[derive(Debug, Serialize)]
pub struct LabTool {
    pub id: &'static str,
    pub installed: bool,
}

fn name_of(id: &str) -> Option<&'static str> {
    TOOLS.iter().find(|(i, _)| *i == id).map(|(_, n)| *n)
}

fn candidates(id: &str, name: &str) -> Vec<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(|d| PathBuf::from(d).join(name).join(format!("{id}.exe")))
            .into_iter()
            .collect()
    } else if cfg!(target_os = "macos") {
        let app = format!("{name}.app");
        let mut v = vec![PathBuf::from("/Applications").join(&app)];
        if let Some(home) = std::env::var_os("HOME") {
            v.push(PathBuf::from(home).join("Applications").join(&app));
        }
        v
    } else {
        vec![PathBuf::from("/usr/bin").join(id)]
    }
}

fn installed_path(id: &str) -> Option<PathBuf> {
    let name = name_of(id)?;
    candidates(id, name).into_iter().find(|p| p.exists())
}

/// The family minus `own`, each with whether it is installed here.
pub fn list(own: &str) -> Vec<LabTool> {
    TOOLS
        .iter()
        .filter(|(id, _)| *id != own)
        .map(|&(id, _)| LabTool {
            id,
            installed: installed_path(id).is_some(),
        })
        .collect()
}

/// Start an installed tool, optionally with one existing file for it to open.
pub fn open(id: &str, file: Option<&Path>) -> Result<(), String> {
    let name = name_of(id).ok_or_else(|| format!("unknown tool {id}"))?;
    let exe = installed_path(id).ok_or_else(|| format!("{name} isn't installed"))?;
    if let Some(f) = file {
        // A real file by full path only: nothing that could pass for an option.
        if !f.is_absolute() || !f.is_file() {
            return Err(format!("{} no longer exists", f.display()));
        }
    }
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg("-a").arg(&exe);
        c
    } else {
        Command::new(&exe)
    };
    if let Some(f) = file {
        cmd.arg(f);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("couldn't start {name}: {e}"))?;
    // Reap it when it exits so no zombie is left behind on macOS/Linux.
    std::thread::spawn(move || {
        if let Err(e) = child.wait() {
            tracing::warn!("waiting for a DeskLab tool failed: {e}");
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_leaves_out_own_tool() {
        let ids: Vec<_> = list("deskfolio").iter().map(|t| t.id).collect();
        assert_eq!(ids, ["deskzero", "deskmedic"]);
    }

    #[test]
    fn unknown_tool_is_refused() {
        assert!(open("notepad", None).unwrap_err().contains("unknown tool"));
    }

    #[test]
    fn relative_or_missing_file_is_refused() {
        // Only reaches the file check when the tool is installed; either way it must fail.
        assert!(open("deskzero", Some(Path::new("--help"))).is_err());
        assert!(open("deskzero", Some(Path::new("/no/such/file.pdf"))).is_err());
    }
}
