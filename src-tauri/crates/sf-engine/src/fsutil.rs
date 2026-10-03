//! Filesystem helpers shared by the planner and the mover: case-aware
//! existence checks and free-name probing for conflict renaming.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Are two paths the same? Case-insensitive comparison on Windows and macOS
/// (their default filesystems are), case-sensitive elsewhere.
pub fn paths_equal(a: &Path, b: &Path) -> bool {
    if cfg!(windows) || cfg!(target_os = "macos") {
        a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

/// Does `candidate` exist, comparing names case-insensitively where the
/// platform demands it? On Windows `try_exists` is already case-insensitive;
/// on macOS/Linux we additionally scan the parent directory so `Report.pdf`
/// is detected when `report.pdf` exists.
pub fn dest_exists(candidate: &Path) -> io::Result<bool> {
    if cfg!(windows) {
        return candidate.try_exists();
    }
    if candidate.try_exists()? {
        return Ok(true);
    }
    let Some(parent) = candidate.parent() else {
        return Ok(false);
    };
    let Some(want) = candidate.file_name().map(|n| n.to_string_lossy().to_lowercase()) else {
        return Ok(false);
    };
    let entries = match fs::read_dir(parent) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name == want {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Find a conflict-free variant of `path` by inserting ` (n)` before the
/// extension: `file.pdf` → `file (1).pdf` → `file (2).pdf`…
/// Returns the original path when it's already free. Extensionless names get
/// `name (1)`. Multi-dot names like `backup.tar.gz` keep their full suffix.
pub fn probe_free_name(path: &Path) -> io::Result<PathBuf> {
    if !dest_exists(path)? {
        return Ok(path.to_path_buf());
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    for n in 1..1000 {
        let candidate_name = match &ext {
            Some(ext) if !ext.is_empty() => format!("{stem} ({n}).{ext}"),
            _ => format!("{stem} ({n})"),
        };
        let candidate = path.with_file_name(OsStr::new(&candidate_name));
        if !dest_exists(&candidate)? {
            return Ok(candidate);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("could not find a free name for {}", path.display()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_inserts_counter_before_extension() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("file.pdf");
        fs::write(&p, b"x").unwrap();

        let free = probe_free_name(&p).unwrap();
        assert_eq!(free.file_name().unwrap(), "file (1).pdf");

        fs::write(&free, b"x").unwrap();
        let free2 = probe_free_name(&p).unwrap();
        assert_eq!(free2.file_name().unwrap(), "file (2).pdf");
    }

    #[test]
    fn probe_handles_extensionless_names() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("README");
        fs::write(&p, b"x").unwrap();
        let free = probe_free_name(&p).unwrap();
        assert_eq!(free.file_name().unwrap(), "README (1)");
    }

    #[test]
    fn probe_keeps_multi_dot_suffixes() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("backup.tar.gz");
        fs::write(&p, b"x").unwrap();
        let free = probe_free_name(&p).unwrap();
        assert_eq!(free.file_name().unwrap(), "backup.tar (1).gz");
    }

    #[test]
    fn free_path_returns_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("new.txt");
        assert_eq!(probe_free_name(&p).unwrap(), p);
    }
}
