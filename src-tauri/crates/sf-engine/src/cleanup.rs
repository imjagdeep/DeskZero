//! "Clean up this folder": things in one folder (not its sub-folders) that
//! are usually safe to throw away. Only suggestions; the browser moves what
//! the user ticks to the Trash, where it can be restored.

use crate::classify::{classify, extension_of};
use crate::duplicates::sha256_file;
use crate::fsutil::is_hidden_or_system;
use crate::types::Category;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Suggestion {
    pub path: PathBuf,
    pub size_bytes: u64,
    /// Why it's suggested, e.g. "copy of report.pdf".
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Group {
    /// unfinished | installers | duplicates | large
    pub id: &'static str,
    pub title: &'static str,
    pub items: Vec<Suggestion>,
}

const DAY: u64 = 86_400;
/// Browser partial-download extensions.
const PARTIAL: &[&str] = &[
    "crdownload",
    "part",
    "partial",
    "download",
    "opdownload",
    "tmp",
];
const LARGE: u64 = 1024 * 1024 * 1024;

struct Entry {
    path: PathBuf,
    name: String,
    size: u64,
    modified: SystemTime,
}

fn older_than(t: SystemTime, days: u64) -> bool {
    SystemTime::now()
        .duration_since(t)
        .is_ok_and(|age| age > Duration::from_secs(days * DAY))
}

/// Suggestions for `dir`, groups in a fixed order, empty groups left out.
pub fn suggest(dir: &Path) -> io::Result<Vec<Group>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        let Ok(md) = entry.metadata() else { continue };
        if !md.is_file() || is_hidden_or_system(&path) {
            continue;
        }
        files.push(Entry {
            name: entry.file_name().to_string_lossy().into_owned(),
            size: md.len(),
            modified: md.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            path,
        });
    }
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let mut unfinished = Vec::new();
    let mut installers = Vec::new();
    let mut large = Vec::new();
    for f in &files {
        let ext = extension_of(&f.name);
        if PARTIAL.contains(&ext.as_str()) && older_than(f.modified, 1) {
            unfinished.push(item(f, "download that never finished"));
        } else if matches!(
            classify(&f.name),
            Category::Installers | Category::DiskImages
        ) && older_than(f.modified, 30)
        {
            installers.push(item(f, "installer older than a month"));
        }
        if f.size >= LARGE {
            large.push(item(f, "over 1 GB"));
        }
    }
    large.sort_by_key(|s| std::cmp::Reverse(s.size_bytes));

    let mut groups = Vec::new();
    for (id, title, items) in [
        ("unfinished", "Unfinished downloads", unfinished),
        ("installers", "Old installers", installers),
        ("duplicates", "Duplicate copies", duplicates(&files)),
        ("large", "Very large files", large),
    ] {
        if !items.is_empty() {
            groups.push(Group { id, title, items });
        }
    }
    Ok(groups)
}

fn item(f: &Entry, reason: &str) -> Suggestion {
    Suggestion {
        path: f.path.clone(),
        size_bytes: f.size,
        reason: reason.to_string(),
    }
}

/// Byte-identical files: the oldest stays (usually the original download),
/// the others are suggested. Only same-size files are hashed.
fn duplicates(files: &[Entry]) -> Vec<Suggestion> {
    let mut by_size: HashMap<u64, Vec<&Entry>> = HashMap::new();
    for f in files.iter().filter(|f| f.size > 0) {
        by_size.entry(f.size).or_default().push(f);
    }
    let mut out = Vec::new();
    for same_size in by_size.values().filter(|v| v.len() > 1) {
        let mut by_hash: HashMap<String, Vec<&Entry>> = HashMap::new();
        for f in same_size {
            if let Ok(h) = sha256_file(&f.path) {
                by_hash.entry(h).or_default().push(f);
            }
        }
        for mut copies in by_hash.into_values().filter(|v| v.len() > 1) {
            copies.sort_by(|a, b| {
                a.modified
                    .cmp(&b.modified)
                    .then(a.name.len().cmp(&b.name.len()))
            });
            let keep = copies[0].name.clone();
            for c in &copies[1..] {
                out.push(item(c, &format!("copy of {keep}")));
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn age(path: &Path, days: u64) {
        let t = SystemTime::now() - Duration::from_secs(days * DAY);
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    #[test]
    fn finds_partials_old_installers_and_copies() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        fs::write(d.join("movie.mp4.crdownload"), b"x").unwrap();
        age(&d.join("movie.mp4.crdownload"), 3);
        fs::write(d.join("fresh.part"), b"x").unwrap(); // still downloading
        fs::write(d.join("setup.exe"), b"installer").unwrap();
        age(&d.join("setup.exe"), 60);
        fs::write(d.join("new-setup.msi"), b"new").unwrap();
        fs::write(d.join("report.pdf"), b"same bytes").unwrap();
        age(&d.join("report.pdf"), 5);
        fs::write(d.join("report (1).pdf"), b"same bytes").unwrap();
        fs::write(d.join("other.pdf"), b"diff bytes").unwrap();

        let groups = suggest(d).unwrap();
        let ids: Vec<&str> = groups.iter().map(|g| g.id).collect();
        assert_eq!(ids, ["unfinished", "installers", "duplicates"]);
        assert!(groups[0].items[0].path.ends_with("movie.mp4.crdownload"));
        assert_eq!(groups[1].items.len(), 1);
        assert!(groups[1].items[0].path.ends_with("setup.exe"));
        assert_eq!(groups[2].items.len(), 1);
        assert!(groups[2].items[0].path.ends_with("report (1).pdf"));
        assert_eq!(groups[2].items[0].reason, "copy of report.pdf");
    }

    #[test]
    fn a_tidy_folder_has_no_suggestions() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("notes.txt"), b"x").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        assert!(suggest(tmp.path()).unwrap().is_empty());
    }
}
