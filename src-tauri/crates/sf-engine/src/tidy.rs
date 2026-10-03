//! Tidying helpers: move duplicate copies aside, archive old files, and a
//! simple storage overview. Like everything else they only *plan*; moves go
//! through the mover so they are recorded and can be undone. Nothing here
//! ever deletes a file.

use crate::fsutil::{free_target, is_hidden_or_system};
use crate::types::{PlanStatus, PlannedOp};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Plan moving `files` into `dest_dir`, keeping their names (conflicts get
/// "name (1).ext", also within the batch).
pub fn plan_move_into(files: &[PathBuf], dest_dir: &Path) -> Vec<PlannedOp> {
    let mut claimed: Vec<PathBuf> = Vec::new();
    let mut ops = Vec::new();
    for src in files {
        let size = fs::symlink_metadata(src).map(|m| m.len()).unwrap_or(0);
        let Some(name) = src.file_name() else {
            continue;
        };
        if !src.is_file() {
            ops.push(skip(src, "not a file", size));
            continue;
        }
        if src.parent() == Some(dest_dir) {
            ops.push(skip(src, "already there", size));
            continue;
        }
        match free_target(&dest_dir.join(name), &claimed) {
            Ok(dst) => {
                claimed.push(dst.clone());
                ops.push(PlannedOp {
                    src: src.clone(),
                    dst,
                    status: PlanStatus::Move { replace: false },
                    rule_id: None,
                    rule_name: None,
                    size_bytes: size,
                });
            }
            Err(e) => ops.push(skip(src, &e.to_string(), size)),
        }
    }
    ops
}

/// Files directly in `folder` not modified for at least `days` days,
/// planned to move into `dest_dir`. Hidden/system files are left alone.
pub fn plan_old_files(folder: &Path, days: u32, dest_dir: &Path) -> io::Result<Vec<PlannedOp>> {
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(u64::from(days) * 86_400))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut old: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(folder)?.flatten() {
        let path = entry.path();
        let Ok(md) = entry.metadata() else { continue };
        if !md.is_file() || is_hidden_or_system(&path) {
            continue;
        }
        if md.modified().map(|m| m < cutoff).unwrap_or(false) {
            old.push(path);
        }
    }
    old.sort();
    Ok(plan_move_into(&old, dest_dir))
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FolderUsage {
    /// Folder name, or "Loose files" for files directly in the root.
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub files: u64,
}

/// Size and file count of each folder directly inside `root` (plus loose
/// files), biggest first. Symlinks are not followed.
pub fn storage_overview(root: &Path) -> io::Result<Vec<FolderUsage>> {
    let mut out = Vec::new();
    let mut loose = FolderUsage {
        name: "Loose files".into(),
        path: root.to_path_buf(),
        bytes: 0,
        files: 0,
    };
    for entry in fs::read_dir(root)?.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let (bytes, files) = folder_size(&path);
            out.push(FolderUsage {
                name: entry.file_name().to_string_lossy().into_owned(),
                path,
                bytes,
                files,
            });
        } else if ft.is_file() && !is_hidden_or_system(&path) {
            loose.bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            loose.files += 1;
        }
    }
    if loose.files > 0 {
        out.push(loose);
    }
    out.sort_by_key(|u| std::cmp::Reverse(u.bytes));
    Ok(out)
}

fn folder_size(dir: &Path) -> (u64, u64) {
    let mut bytes = 0;
    let mut files = 0;
    for e in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        if e.file_type().is_file() {
            bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
            files += 1;
        }
    }
    (bytes, files)
}

fn skip(src: &Path, reason: &str, size: u64) -> PlannedOp {
    PlannedOp {
        src: src.to_path_buf(),
        dst: src.to_path_buf(),
        status: PlanStatus::Skip {
            reason: reason.into(),
        },
        rule_id: None,
        rule_name: None,
        size_bytes: size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_into_resolves_name_clashes_within_the_batch() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("photo.jpg"), b"1").unwrap();
        fs::write(b.join("photo.jpg"), b"1").unwrap();
        let dest = tmp.path().join("Duplicates");
        let ops = plan_move_into(&[a.join("photo.jpg"), b.join("photo.jpg")], &dest);
        assert_eq!(ops[0].dst, dest.join("photo.jpg"));
        assert_eq!(ops[1].dst, dest.join("photo (1).jpg"));
    }

    #[test]
    fn old_files_respects_age_and_skips_system_files() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("new.txt"), b"x").unwrap();
        fs::write(tmp.path().join("desktop.ini"), b"x").unwrap();
        let dest = tmp.path().join("Archive");
        // Nothing is older than 1 day yet.
        assert!(plan_old_files(tmp.path(), 1, &dest).unwrap().is_empty());
        // With 0 days everything (except system files) qualifies.
        let ops = plan_old_files(tmp.path(), 0, &dest).unwrap();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].src.ends_with("new.txt"));
    }

    #[test]
    fn storage_overview_counts_folders_and_loose_files() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("Images/2026")).unwrap();
        fs::write(tmp.path().join("Images/2026/a.jpg"), vec![0u8; 300]).unwrap();
        fs::write(tmp.path().join("Images/b.jpg"), vec![0u8; 200]).unwrap();
        fs::write(tmp.path().join("loose.txt"), vec![0u8; 10]).unwrap();
        let usage = storage_overview(tmp.path()).unwrap();
        assert_eq!(usage[0].name, "Images");
        assert_eq!((usage[0].bytes, usage[0].files), (500, 2));
        assert_eq!(usage[1].name, "Loose files");
        assert_eq!(usage[1].files, 1);
    }
}
