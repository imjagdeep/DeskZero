//! Operation history: append-only JSONL, one batch per line, with undo.
//!
//! Crash-safe by construction: appends are flushed per line, a torn last
//! line is ignored on read, and compaction writes-then-renames. Entries older
//! than `keep_history_days` or a file over 5 MB trigger compaction on load.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

const MAX_HISTORY_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryKind {
    Move,
    Undo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryItem {
    pub src: PathBuf,
    pub dst: PathBuf,
}

/// One recorded batch. `items` are the successful operations (what undo
/// reverses, in execution order); `failed` stayed at their source;
/// `quarantined` maps a replaced destination to where its old content was
/// parked (undo restores it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub ts: DateTime<Utc>,
    pub kind: HistoryKind,
    pub items: Vec<HistoryItem>,
    #[serde(default)]
    pub quarantined: Vec<HistoryItem>,
    #[serde(default)]
    pub failed: Vec<FailedItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailedItem {
    pub src: PathBuf,
    pub error: String,
}

/// Monotonic-enough operation id without a uuid dependency:
/// `op_<unix_millis>_<pid>_<counter>`.
pub fn new_op_id(counter: u64) -> String {
    format!(
        "op_{}_{}_{}",
        Utc::now().timestamp_millis(),
        std::process::id(),
        counter
    )
}

/// Append an entry and flush. Failure to write history must not lose the
/// moves that already happened — callers log loudly instead of erroring out.
pub fn append(path: &Path, entry: &HistoryEntry) -> io::Result<()> {
    let line = serde_json::to_string(entry)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{line}")?;
    f.flush()
}

/// Read all entries, ignoring a torn last line. Returns newest first.
pub fn read(path: &Path) -> Vec<HistoryEntry> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            tracing::warn!("cannot open history {}: {e}", path.display());
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for line in BufReader::new(file).lines() {
        match line {
            Ok(text) if text.trim().is_empty() => continue,
            Ok(text) => match serde_json::from_str::<HistoryEntry>(&text) {
                Ok(e) => out.push(e),
                Err(e) => tracing::warn!("ignoring torn history line: {e}"),
            },
            Err(e) => {
                tracing::warn!("history read interrupted: {e}");
                break;
            }
        }
    }
    out.reverse(); // newest first
    out
}

/// Drop entries older than `keep_days` and rewrite compactly. Also compacts
/// when the file exceeds MAX_HISTORY_BYTES regardless of age.
pub fn compact(path: &Path, keep_days: u32) -> io::Result<()> {
    let Ok(md) = fs::metadata(path) else {
        return Ok(()); // nothing to compact
    };
    let entries = read(path);
    if entries.is_empty() {
        return Ok(());
    }
    let cutoff = Utc::now() - chrono::Duration::days(keep_days as i64);
    let needs_age_cut = entries.last().map(|e| e.ts < cutoff).unwrap_or(false);
    if md.len() <= MAX_HISTORY_BYTES && !needs_age_cut {
        return Ok(());
    }
    let keep: Vec<&HistoryEntry> = entries.iter().filter(|e| e.ts >= cutoff).collect();
    let tmp = path.with_extension("compact.tmp");
    {
        let mut f = File::create(&tmp)?;
        for e in &keep {
            let line = serde_json::to_string(e)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            writeln!(f, "{line}")?;
        }
        f.flush()?;
    }
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(_) => {
            let _ = fs::remove_file(path);
            fs::rename(&tmp, path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn entry(id: &str, ts: DateTime<Utc>) -> HistoryEntry {
        HistoryEntry {
            id: id.into(),
            ts,
            kind: HistoryKind::Move,
            items: vec![HistoryItem {
                src: PathBuf::from(format!("/a/{id}")),
                dst: PathBuf::from(format!("/b/{id}")),
            }],
            quarantined: Vec::new(),
            failed: Vec::new(),
        }
    }

    #[test]
    fn append_then_read_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("history.jsonl");
        append(&p, &entry("one", Utc::now() - chrono::Duration::days(2))).unwrap();
        append(&p, &entry("two", Utc::now())).unwrap();

        let entries = read(&p);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "two");
        assert_eq!(entries[1].id, "one");
    }

    #[test]
    fn torn_last_line_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("history.jsonl");
        append(&p, &entry("good", Utc::now())).unwrap();
        // Simulate a mid-write crash: append half of a second line.
        let mut f = OpenOptions::new().append(true).open(&p).unwrap();
        f.write_all(b"{\"id\":\"bad\"").unwrap();
        f.flush().unwrap();
        drop(f);

        let entries = read(&p);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "good");
    }

    #[test]
    fn compact_drops_old_entries() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("history.jsonl");
        append(&p, &entry("ancient", Utc::now() - chrono::Duration::days(200))).unwrap();
        append(&p, &entry("fresh", Utc::now())).unwrap();

        compact(&p, 90).unwrap();
        let entries = read(&p);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "fresh");
    }

    #[test]
    fn compact_noop_when_small_and_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("history.jsonl");
        append(&p, &entry("fresh", Utc::now())).unwrap();
        let before = fs::metadata(&p).unwrap().len();
        compact(&p, 90).unwrap();
        assert_eq!(fs::metadata(&p).unwrap().len(), before);
    }
}
