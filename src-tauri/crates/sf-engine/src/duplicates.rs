//! Duplicate finder: groups files by size, then confirms with SHA-256.
//!
//! Only size-colliding files are hashed (8 MB streaming reads), so a folder
//! of distinct files costs nothing but stat calls. This module NEVER deletes
//! or moves anything — it reports; the user decides.

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq)]
pub struct DupFile {
    pub path: PathBuf,
    pub modified: Option<std::time::SystemTime>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DuplicateGroup {
    pub size_bytes: u64,
    pub sha256: String,
    pub files: Vec<DupFile>,
}

const HASH_CHUNK: usize = 8 * 1024 * 1024;

/// Scan `root` recursively (symlinks never followed) and return groups of
/// two or more byte-identical files.
pub fn find_duplicates(root: &Path) -> io::Result<Vec<DuplicateGroup>> {
    // Pass 1: group by size.
    let mut by_size: Vec<(u64, Vec<PathBuf>)> = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue; // skips dirs and symlinks alike
        }
        let md = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("dupes: stat failed {}: {e}", entry.path().display());
                continue;
            }
        };
        let size = md.len();
        match by_size.iter_mut().find(|(s, _)| *s == size) {
            Some((_, paths)) => paths.push(entry.path().to_path_buf()),
            None => by_size.push((size, vec![entry.path().to_path_buf()])),
        }
    }

    // Pass 2: hash only the size-colliders.
    let mut groups = Vec::new();
    for (size, paths) in by_size.into_iter().filter(|(_, p)| p.len() > 1) {
        let mut by_hash: Vec<(String, Vec<DupFile>)> = Vec::new();
        for path in paths {
            match sha256_file(&path) {
                Ok(hash) => {
                    let modified = fs_modified(&path);
                    match by_hash.iter_mut().find(|(h, _)| *h == hash) {
                        Some((_, files)) => files.push(DupFile { path, modified }),
                        None => by_hash.push((hash, vec![DupFile { path, modified }])),
                    }
                }
                Err(e) => tracing::warn!("dupes: hash failed {}: {e}", path.display()),
            }
        }
        for (hash, files) in by_hash.into_iter().filter(|(_, f)| f.len() > 1) {
            groups.push(DuplicateGroup {
                size_bytes: size,
                sha256: hash,
                files,
            });
        }
    }

    groups.sort_by_key(|g| std::cmp::Reverse(g.size_bytes)); // biggest first
    Ok(groups)
}

/// Lowercase hex SHA-256 of a file, streamed in HASH_CHUNK-sized reads.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; HASH_CHUNK];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn fs_modified(path: &Path) -> Option<std::time::SystemTime> {
    path.metadata().ok().and_then(|m| m.modified().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_identical_files_regardless_of_name() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("photo.jpg"), b"same-bytes").unwrap();
        fs::write(root.join("photo (1).jpg"), b"same-bytes").unwrap();
        fs::write(root.join("IMG_1234.jpg"), b"same-bytes").unwrap();
        fs::write(root.join("other.jpg"), b"different").unwrap();
        // Same size as the colliders but different content: must not match.
        fs::write(root.join("imposter.jpg"), b"same-byteZ").unwrap();

        let groups = find_duplicates(root).unwrap();
        assert_eq!(groups.len(), 1);
        let g = &groups[0];
        assert_eq!(g.files.len(), 3);
        assert_eq!(g.size_bytes, 10);
        assert!(!g.files.iter().any(|f| f.path.ends_with("imposter.jpg")));
    }

    #[test]
    fn no_false_positives_when_sizes_differ() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("a.txt"), b"short").unwrap();
        fs::write(root.join("b.txt"), b"much longer content").unwrap();
        assert!(find_duplicates(root).unwrap().is_empty());
    }

    #[test]
    fn empty_dirs_are_fine() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(find_duplicates(tmp.path()).unwrap().is_empty());
    }

    #[test]
    fn hashes_are_stable_hex() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("x.bin");
        fs::write(&p, b"abc").unwrap();
        let h = sha256_file(&p).unwrap();
        assert_eq!(h.len(), 64);
        assert_eq!(
            h,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
