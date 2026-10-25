//! Everyday file-manager actions for the browser: rename, new folder, copy,
//! move a folder, and move to the Trash / Recycle Bin. Moving *files* goes
//! through the planner and mover instead, so it lands in History and can be
//! undone. Nothing here deletes permanently.

use crate::fsutil::free_target;
use crate::types::{PlanStatus, PlannedOp};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A file or folder name the user typed: not empty, no path separators and
/// nothing Windows refuses (so a name also works when the folder is synced
/// to a Windows machine).
pub fn validate_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("The name can't be empty.".into());
    }
    if trimmed == "." || trimmed == ".." {
        return Err("That name is reserved.".into());
    }
    if let Some(c) = name.chars().find(|&c| {
        matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()
    }) {
        return Err(format!("A name can't contain {c:?}."));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err("A name can't end with a dot or a space.".into());
    }
    let stem = trimmed
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"];
    let numbered = (stem.starts_with("COM") || stem.starts_with("LPT"))
        && stem.len() == 4
        && stem.as_bytes()[3].is_ascii_digit();
    if reserved.contains(&stem.as_str()) || numbered {
        return Err(format!("{stem} is a reserved name on Windows."));
    }
    Ok(())
}

fn invalid(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, msg)
}

/// Rename `path` within its folder. Refuses to replace something that
/// already has the new name (a case-only change is allowed).
pub fn rename(path: &Path, new_name: &str) -> io::Result<PathBuf> {
    validate_name(new_name).map_err(invalid)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid(format!("{} can't be renamed", path.display())))?;
    let target = parent.join(new_name);
    let case_only = path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().to_lowercase() == new_name.to_lowercase());
    if !case_only && fs::symlink_metadata(&target).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("\"{new_name}\" already exists here"),
        ));
    }
    fs::rename(path, &target)?;
    Ok(target)
}

/// Create a folder named `base` in `dir`, or "base 2", "base 3"… when taken
/// (Finder and GNOME style; Explorer's "(2)" reads the same).
pub fn new_folder(dir: &Path, base: &str) -> io::Result<PathBuf> {
    validate_name(base).map_err(invalid)?;
    for n in 1..1000 {
        let name = if n == 1 {
            base.to_string()
        } else {
            format!("{base} {n}")
        };
        let path = dir.join(name);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(invalid("too many folders with that name".into()))
}

/// What happened to each item of a copy or move.
#[derive(Debug, Default, Clone, PartialEq, serde::Serialize)]
pub struct Outcome {
    pub done: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
}

/// Copy files and folders into `dest`. Name clashes (also pasting into the
/// same folder) get "name (1)". Symlinks inside folders are skipped.
pub fn copy_into(srcs: &[PathBuf], dest: &Path) -> Outcome {
    let mut out = Outcome::default();
    for src in srcs {
        match copy_one(src, dest) {
            Ok(p) => out.done.push(p),
            Err(e) => out.failed.push((src.clone(), e.to_string())),
        }
    }
    out
}

fn copy_one(src: &Path, dest: &Path) -> io::Result<PathBuf> {
    let name = src
        .file_name()
        .ok_or_else(|| invalid(format!("{} can't be copied", src.display())))?;
    if src.is_dir() && dest.starts_with(src) {
        return Err(invalid("a folder can't be copied into itself".into()));
    }
    let target = free_target(&dest.join(name), &[])?;
    if src.is_dir() {
        copy_dir(src, &target)?;
    } else {
        fs::copy(src, &target)?;
    }
    Ok(target)
}

fn copy_dir(src: &Path, target: &Path) -> io::Result<()> {
    for entry in walkdir::WalkDir::new(src).follow_links(false) {
        let entry = entry.map_err(io::Error::other)?;
        let rel = entry
            .path()
            .strip_prefix(src)
            .map_err(|e| invalid(e.to_string()))?;
        let to = target.join(rel);
        let ft = entry.file_type();
        if ft.is_dir() {
            fs::create_dir_all(&to)?;
        } else if ft.is_file() {
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Plan moving files and folders into `dest` for the mover (which records
/// History, so the move can be undone). Name clashes get "name (1)"; a
/// folder never goes inside itself; items already in `dest` stay put.
pub fn plan_move(srcs: &[PathBuf], dest: &Path) -> Vec<PlannedOp> {
    let mut claimed: Vec<PathBuf> = Vec::new();
    srcs.iter()
        .map(|src| {
            let size = fs::symlink_metadata(src).map(|m| m.len()).unwrap_or(0);
            let status = |status| PlannedOp {
                src: src.clone(),
                dst: src.clone(),
                status,
                rule_id: None,
                rule_name: None,
                size_bytes: size,
            };
            let Some(name) = src.file_name() else {
                return status(PlanStatus::Skip {
                    reason: "can't be moved".into(),
                });
            };
            if src.parent() == Some(dest) {
                return status(PlanStatus::Noop);
            }
            if src.is_dir() && dest.starts_with(src) {
                return status(PlanStatus::Skip {
                    reason: "a folder can't be moved into itself".into(),
                });
            }
            match free_target(&dest.join(name), &claimed) {
                Ok(dst) => {
                    claimed.push(dst.clone());
                    PlannedOp {
                        dst,
                        ..status(PlanStatus::Move { replace: false })
                    }
                }
                Err(e) => status(PlanStatus::Skip {
                    reason: e.to_string(),
                }),
            }
        })
        .collect()
}

/// Move items to the Trash / Recycle Bin, where they can be restored.
pub fn trash(paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    #[allow(unused_mut)]
    let mut ctx = trash::TrashContext::default();
    // macOS: the system call, not scripting Finder, which would ask the
    // user to let DeskZero control Finder.
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        ctx.set_delete_method(DeleteMethod::NsFileManager);
    }
    ctx.delete_all(paths)
        .map_err(|e| format!("couldn't move to the Trash: {e}"))
}

/// Put items back from the Trash / Recycle Bin to where they were (undo of
/// `trash`). The most recently deleted item wins when the same path was
/// deleted twice. Returns how many were restored.
#[cfg(not(target_os = "macos"))]
pub fn restore_from_trash(paths: &[PathBuf]) -> Result<usize, String> {
    let all = trash::os_limited::list().map_err(|e| format!("couldn't read the Trash: {e}"))?;
    let mut picked: Vec<trash::TrashItem> = Vec::new();
    for want in paths {
        let newest = all
            .iter()
            .filter(|it| crate::fsutil::paths_equal(&it.original_path(), want))
            .max_by_key(|it| it.time_deleted);
        if let Some(it) = newest {
            picked.push(it.clone());
        }
    }
    let n = picked.len();
    trash::os_limited::restore_all(picked).map_err(|e| format!("couldn't put it back: {e}"))?;
    Ok(n)
}

/// macOS gives apps no way to put Trash items back; Finder's "Put Back" does.
#[cfg(target_os = "macos")]
pub fn restore_from_trash(_paths: &[PathBuf]) -> Result<usize, String> {
    Err("On a Mac, open the Trash in Finder and choose Put Back.".into())
}

/// Remove a folder only if it is empty (undo of `new_folder`).
pub fn remove_empty_folder(path: &Path) -> io::Result<()> {
    fs::remove_dir(path)
}

/// The start of a text file for the preview, or None for binary files.
pub fn text_preview(path: &Path, max_bytes: usize) -> io::Result<Option<String>> {
    use std::io::Read;
    let mut buf = Vec::new();
    fs::File::open(path)?
        .take(max_bytes as u64)
        .read_to_end(&mut buf)?;
    if buf.iter().take(8192).any(|&b| b == 0) {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_preview_reads_text_and_skips_binary() {
        let tmp = tempfile::tempdir().unwrap();
        let txt = tmp.path().join("a.txt");
        fs::write(&txt, "hello world").unwrap();
        assert_eq!(text_preview(&txt, 5).unwrap().as_deref(), Some("hello"));
        let bin = tmp.path().join("a.bin");
        fs::write(&bin, [1u8, 0, 2, 3]).unwrap();
        assert_eq!(text_preview(&bin, 100).unwrap(), None);
    }

    #[test]
    fn remove_empty_folder_keeps_folders_with_content() {
        let tmp = tempfile::tempdir().unwrap();
        let full = tmp.path().join("full");
        fs::create_dir(&full).unwrap();
        fs::write(full.join("x"), b"x").unwrap();
        assert!(remove_empty_folder(&full).is_err());
        assert!(full.join("x").exists());
        let empty = tmp.path().join("empty");
        fs::create_dir(&empty).unwrap();
        remove_empty_folder(&empty).unwrap();
        assert!(!empty.exists());
    }

    #[test]
    fn names_are_checked() {
        assert!(validate_name("report 2026.pdf").is_ok());
        assert!(validate_name("  ").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("what?").is_err());
        assert!(validate_name("trailing.").is_err());
        assert!(validate_name("con.txt").is_err());
        assert!(validate_name("COM1").is_err());
        assert!(validate_name("console.txt").is_ok());
    }

    #[test]
    fn rename_refuses_to_replace_but_allows_case_change() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.txt");
        fs::write(&a, b"a").unwrap();
        fs::write(tmp.path().join("b.txt"), b"b").unwrap();
        assert!(rename(&a, "b.txt").is_err());
        assert_eq!(fs::read(tmp.path().join("b.txt")).unwrap(), b"b");
        let upper = rename(&a, "A.txt").unwrap();
        assert!(upper.ends_with("A.txt"));
        assert!(rename(&upper, "c.txt").is_ok());
        assert!(tmp.path().join("c.txt").exists());
    }

    #[test]
    fn new_folder_counts_up() {
        let tmp = tempfile::tempdir().unwrap();
        let one = new_folder(tmp.path(), "New folder").unwrap();
        let two = new_folder(tmp.path(), "New folder").unwrap();
        assert!(one.ends_with("New folder"));
        assert!(two.ends_with("New folder 2"));
    }

    #[test]
    fn copy_into_copies_folders_and_avoids_clashes() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("inner")).unwrap();
        fs::write(src.join("inner/x.txt"), b"x").unwrap();
        fs::write(tmp.path().join("f.txt"), b"f").unwrap();
        let dest = tmp.path().join("dest");
        fs::create_dir_all(&dest).unwrap();

        let out = copy_into(&[src.clone(), tmp.path().join("f.txt")], &dest);
        assert!(out.failed.is_empty(), "{:?}", out.failed);
        assert_eq!(fs::read(dest.join("src/inner/x.txt")).unwrap(), b"x");
        assert!(dest.join("f.txt").exists());
        assert!(src.join("inner/x.txt").exists(), "copy keeps the original");

        // Pasting the same file again keeps both.
        let again = copy_into(&[tmp.path().join("f.txt")], &dest);
        assert!(again.done[0].ends_with("f (1).txt"));

        // Never into itself.
        let inside = copy_into(std::slice::from_ref(&src), &src.join("inner"));
        assert_eq!(inside.failed.len(), 1);
    }

    #[test]
    fn plan_move_handles_folders_clashes_and_self_nesting() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        fs::create_dir_all(a.join("deep")).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(b.join("x.txt"), b"old").unwrap();
        fs::write(tmp.path().join("x.txt"), b"new").unwrap();

        let ops = plan_move(&[a.clone(), tmp.path().join("x.txt")], &b);
        assert_eq!(ops[0].status, PlanStatus::Move { replace: false });
        assert_eq!(ops[0].dst, b.join("a"));
        assert_eq!(ops[1].dst, b.join("x (1).txt"));

        let nested = plan_move(std::slice::from_ref(&a), &a.join("deep"));
        assert!(matches!(nested[0].status, PlanStatus::Skip { .. }));
        let same = plan_move(&[b.join("x.txt")], &b);
        assert_eq!(same[0].status, PlanStatus::Noop);
    }
}
