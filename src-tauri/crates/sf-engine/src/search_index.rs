//! Universal search: an in-memory index of file, folder and app names that
//! answers as you type (Spotlight-style). Built by walking the user's
//! folders once in the background; queries never touch the disk.
//!
//! Apps come from the platform's launcher folders: Start Menu shortcuts on
//! Windows, `.app` bundles on macOS, `.desktop` entries on Linux.

use crate::fsutil::is_hidden_or_system;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    App,
    Folder,
    File,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Entry {
    pub path: PathBuf,
    /// Display name (app name for apps, file name otherwise).
    pub name: String,
    pub kind: EntryKind,
    #[serde(skip)]
    name_lower: String,
    #[serde(skip)]
    modified: Option<SystemTime>,
}

#[derive(Debug, Default)]
pub struct Index {
    entries: Vec<Entry>,
}

/// Folders never worth indexing (huge, generated or internal).
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "__pycache__",
    "venv",
    ".venv",
    "appdata",
    "$recycle.bin",
    "system volume information",
];

const MAX_DEPTH: usize = 14;

impl Index {
    /// Walk `file_roots` (recursively) and `app_roots`; stop at `limit`
    /// entries so a giant drive can't eat memory.
    pub fn build(file_roots: &[PathBuf], app_roots: &[PathBuf], limit: usize) -> Index {
        let mut entries = Vec::new();
        let mut seen_apps: HashSet<String> = HashSet::new();
        for root in app_roots {
            collect_apps(root, &mut entries, &mut seen_apps);
        }
        let mut seen_roots: Vec<PathBuf> = Vec::new();
        for root in file_roots {
            // Skip roots already covered by an earlier root.
            if seen_roots.iter().any(|r| root.starts_with(r)) || !root.is_dir() {
                continue;
            }
            seen_roots.push(root.clone());
            collect_files(root, &mut entries, limit);
            if entries.len() >= limit {
                break;
            }
        }
        Index { entries }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Best matches for `query`: every word must appear in the name.
    /// Ranking: exact name, then prefix, then word start, then anywhere;
    /// apps first on ties, shorter names and newer files next.
    pub fn query(&self, query: &str, limit: usize) -> Vec<Entry> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let words: Vec<&str> = q.split_whitespace().collect();
        let mut scored: Vec<(u32, &Entry)> = self
            .entries
            .iter()
            .filter(|e| words.iter().all(|w| e.name_lower.contains(w)))
            .map(|e| (score(e, &q, &words), e))
            .collect();
        scored.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| b.1.modified.cmp(&a.1.modified))
                .then_with(|| a.1.path.as_os_str().len().cmp(&b.1.path.as_os_str().len()))
        });
        scored
            .into_iter()
            .take(limit)
            .map(|(_, e)| e.clone())
            .collect()
    }
}

fn score(e: &Entry, q: &str, words: &[&str]) -> u32 {
    let name = &e.name_lower;
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    let mut s = if name == q || stem == q {
        0
    } else if name.starts_with(q) {
        100
    } else if words.iter().all(|w| starts_word(name, w)) {
        200
    } else {
        300
    };
    s += match e.kind {
        EntryKind::App => 0,
        EntryKind::Folder => 20,
        EntryKind::File => 30,
    };
    s + (name.chars().count() as u32).min(60)
}

/// Does `word` start at a word boundary in `name` (start, or after a
/// space, `-`, `_`, `.` or `(`)?
fn starts_word(name: &str, word: &str) -> bool {
    let mut from = 0;
    while let Some(i) = name[from..].find(word) {
        let at = from + i;
        let boundary = at == 0
            || name[..at]
                .chars()
                .last()
                .is_some_and(|c| matches!(c, ' ' | '-' | '_' | '.' | '('));
        if boundary {
            return true;
        }
        from = at + word.len().max(1);
        if from >= name.len() {
            break;
        }
    }
    false
}

fn collect_files(root: &Path, entries: &mut Vec<Entry>, limit: usize) {
    let walker = WalkDir::new(root)
        .follow_links(false)
        .max_depth(MAX_DEPTH)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_string_lossy().to_lowercase();
            if e.file_type().is_dir() && SKIP_DIRS.contains(&name.as_str()) {
                return false;
            }
            !is_hidden_or_system(e.path())
        });
    for e in walker.flatten() {
        if e.depth() == 0 {
            continue;
        }
        let ft = e.file_type();
        let kind = if ft.is_dir() {
            EntryKind::Folder
        } else if ft.is_file() {
            EntryKind::File
        } else {
            continue;
        };
        let name = e.file_name().to_string_lossy().into_owned();
        let modified = e.metadata().ok().and_then(|m| m.modified().ok());
        entries.push(Entry {
            name_lower: name.to_lowercase(),
            name,
            path: e.into_path(),
            kind,
            modified,
        });
        if entries.len() >= limit {
            return;
        }
    }
}

fn collect_apps(root: &Path, entries: &mut Vec<Entry>, seen: &mut HashSet<String>) {
    let mut it = WalkDir::new(root)
        .follow_links(false)
        .max_depth(5)
        .into_iter();
    while let Some(next) = it.next() {
        let Ok(e) = next else { continue };
        let path = e.path().to_path_buf();
        let ext = path
            .extension()
            .map(|x| x.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let is_dir = e.file_type().is_dir();
        let name = match ext.as_str() {
            "lnk" if !is_dir => file_stem(&path),
            "app" if is_dir => {
                // A macOS .app bundle is an app, not a folder to walk into.
                it.skip_current_dir();
                file_stem(&path)
            }
            "desktop" if !is_dir => match desktop_entry_name(&path) {
                Some(n) => n,
                None => continue,
            },
            _ => continue,
        };
        let lower = name.to_lowercase();
        if lower.contains("uninstall") || !seen.insert(lower.clone()) {
            continue;
        }
        entries.push(Entry {
            path,
            name,
            kind: EntryKind::App,
            name_lower: lower,
            modified: None,
        });
    }
}

fn file_stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `Name=` from a Linux .desktop file; None for hidden entries.
fn desktop_entry_name(p: &Path) -> Option<String> {
    let text = std::fs::read_to_string(p).ok()?;
    let mut name = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line != "[Desktop Entry]" && name.is_some() {
            break;
        }
        if line.eq_ignore_ascii_case("NoDisplay=true") || line.eq_ignore_ascii_case("Hidden=true") {
            return None;
        }
        if name.is_none() {
            if let Some(v) = line.strip_prefix("Name=") {
                name = Some(v.trim().to_string());
            }
        }
    }
    name.filter(|n| !n.is_empty())
}

/// The usual places people keep their files.
pub fn default_file_roots() -> Vec<PathBuf> {
    [
        dirs::desktop_dir(),
        dirs::document_dir(),
        dirs::download_dir(),
        dirs::picture_dir(),
        dirs::audio_dir(),
        dirs::video_dir(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Where installed apps are listed on this platform.
pub fn default_app_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(windows) {
        if let Some(d) = dirs::data_dir() {
            roots.push(d.join("Microsoft\\Windows\\Start Menu\\Programs"));
        }
        if let Some(pd) = std::env::var_os("ProgramData") {
            roots.push(PathBuf::from(pd).join("Microsoft\\Windows\\Start Menu\\Programs"));
        }
    } else if cfg!(target_os = "macos") {
        roots.push(PathBuf::from("/Applications"));
        roots.push(PathBuf::from("/System/Applications"));
        if let Some(h) = dirs::home_dir() {
            roots.push(h.join("Applications"));
        }
    } else {
        roots.push(PathBuf::from("/usr/share/applications"));
        roots.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
        if let Some(d) = dirs::data_dir() {
            roots.push(d.join("applications"));
        }
    }
    roots.retain(|r| r.is_dir());
    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        fs::create_dir_all(r.join("Work/Invoices")).unwrap();
        fs::create_dir_all(r.join("node_modules/pkg")).unwrap();
        fs::write(r.join("Work/Invoices/invoice-july.pdf"), b"x").unwrap();
        fs::write(r.join("Work/old_invoice.pdf"), b"x").unwrap();
        fs::write(r.join("Work/report.docx"), b"x").unwrap();
        fs::write(r.join("node_modules/pkg/invoice.js"), b"x").unwrap();
        fs::write(r.join(".hidden-invoice"), b"x").unwrap();
        tmp
    }

    #[test]
    fn finds_files_and_folders_and_skips_junk() {
        let tmp = tree();
        let idx = Index::build(&[tmp.path().to_path_buf()], &[], 1000);
        let hits = idx.query("invoice", 10);
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"Invoices"));
        assert!(names.contains(&"invoice-july.pdf"));
        assert!(names.contains(&"old_invoice.pdf"));
        assert!(!names
            .iter()
            .any(|n| *n == "invoice.js" || n.starts_with('.')));
    }

    #[test]
    fn prefix_matches_rank_before_inner_matches() {
        let tmp = tree();
        let idx = Index::build(&[tmp.path().to_path_buf()], &[], 1000);
        let hits = idx.query("invoice", 10);
        let pos = |n: &str| hits.iter().position(|h| h.name == n).unwrap();
        assert!(pos("invoice-july.pdf") < pos("old_invoice.pdf"));
    }

    #[test]
    fn all_words_must_match() {
        let tmp = tree();
        let idx = Index::build(&[tmp.path().to_path_buf()], &[], 1000);
        assert_eq!(idx.query("invoice july", 10).len(), 1);
        assert!(idx.query("invoice zzz", 10).is_empty());
    }

    #[test]
    fn apps_are_found_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let start = tmp.path().join("Programs/Tools");
        fs::create_dir_all(&start).unwrap();
        fs::write(start.join("Calculator.lnk"), b"x").unwrap();
        fs::write(start.join("Uninstall Thing.lnk"), b"x").unwrap();
        let idx = Index::build(&[], &[tmp.path().to_path_buf()], 1000);
        let hits = idx.query("calc", 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, EntryKind::App);
        assert!(idx.query("uninstall", 5).is_empty());
    }

    #[test]
    fn limit_caps_memory() {
        let tmp = tree();
        let idx = Index::build(&[tmp.path().to_path_buf()], &[], 2);
        assert_eq!(idx.len(), 2);
    }
}
