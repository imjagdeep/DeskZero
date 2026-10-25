//! Read-only folder listing for the file browser: what's in a folder, the
//! category of each file, and the user's usual places (Downloads, Desktop…).
//! Nothing here moves or changes a file.

use crate::classify::classify;
use crate::fsutil::is_hidden_or_system;
use crate::types::Category;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// File size; 0 for folders (see `folder_size` for those).
    pub size_bytes: u64,
    pub modified: Option<SystemTime>,
    /// Files only.
    pub category: Option<Category>,
    /// Folders only: entries directly inside (hidden ones not counted), or
    /// None when the folder can't be read.
    pub children: Option<u64>,
}

/// Everything directly inside `dir`, folders first, then by name
/// (case-insensitive). Hidden and system entries are left out unless
/// `show_hidden`. Symlinks are listed as what they point to.
pub fn list_dir(dir: &Path, show_hidden: bool) -> io::Result<Vec<Item>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if !show_hidden && is_hidden_or_system(&path) {
            continue;
        }
        // Broken links and entries we may not stat are skipped, like Explorer.
        let Ok(md) = fs::metadata(&path) else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = md.is_dir();
        out.push(Item {
            category: (!is_dir).then(|| classify(&name)),
            children: is_dir.then(|| count_children(&path, show_hidden)).flatten(),
            size_bytes: if is_dir { 0 } else { md.len() },
            modified: md.modified().ok(),
            is_dir,
            path,
            name,
        });
    }
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

fn count_children(dir: &Path, show_hidden: bool) -> Option<u64> {
    let rd = fs::read_dir(dir).ok()?;
    Some(
        rd.flatten()
            .filter(|e| show_hidden || !is_hidden_or_system(&e.path()))
            .count() as u64,
    )
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Place {
    /// Stable id the UI picks an icon by: home, desktop, documents,
    /// downloads, pictures, music, videos.
    pub id: &'static str,
    pub label: String,
    pub path: PathBuf,
}

/// The user's standard folders that exist on this computer, in sidebar
/// order. Uses the OS's own idea of each (e.g. a Desktop moved to OneDrive).
pub fn places() -> Vec<Place> {
    let candidates: [(&'static str, &str, Option<PathBuf>); 7] = [
        ("desktop", "Desktop", dirs::desktop_dir()),
        ("documents", "Documents", dirs::document_dir()),
        ("downloads", "Downloads", dirs::download_dir()),
        ("pictures", "Pictures", dirs::picture_dir()),
        ("music", "Music", dirs::audio_dir()),
        ("videos", "Videos", dirs::video_dir()),
        ("home", "Home", dirs::home_dir()),
    ];
    candidates
        .into_iter()
        .filter_map(|(id, label, path)| {
            let path = path.filter(|p| p.is_dir())?;
            // Home shows the user's name, as Finder and Files do.
            let label = if id == "home" {
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| label.to_string())
            } else {
                label.to_string()
            };
            Some(Place { id, label, path })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folders_first_with_categories_and_counts() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("zeta/inner")).unwrap();
        fs::write(tmp.path().join("zeta/a.txt"), b"x").unwrap();
        fs::write(tmp.path().join("zeta/.hidden"), b"x").unwrap();
        fs::write(tmp.path().join("Beta.PDF"), vec![0u8; 42]).unwrap();
        fs::write(tmp.path().join("alpha.mp4"), b"x").unwrap();
        fs::write(tmp.path().join("desktop.ini"), b"x").unwrap();

        let items = list_dir(tmp.path(), false).unwrap();
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["zeta", "alpha.mp4", "Beta.PDF"]);

        assert!(items[0].is_dir);
        assert_eq!(items[0].children, Some(2));
        assert_eq!(items[0].category, None);
        assert_eq!(items[1].category, Some(Category::Videos));
        assert_eq!(items[2].category, Some(Category::Documents));
        assert_eq!(items[2].size_bytes, 42);
    }

    #[test]
    fn show_hidden_includes_hidden_entries() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("d")).unwrap();
        fs::write(tmp.path().join("d/.env"), b"x").unwrap();
        fs::write(tmp.path().join(".gitignore"), b"x").unwrap();
        let items = list_dir(tmp.path(), true).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].children, Some(1));
    }

    #[test]
    fn missing_folder_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(list_dir(&tmp.path().join("nope"), false).is_err());
    }
}
