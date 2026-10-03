//! The mover: executes a plan safely.
//!
//! Guarantees:
//! - never deletes anything (replace quarantines the old file instead)
//! - retries locked files with backoff before giving up
//! - falls back to copy+remove across volumes (rename can't span drives)
//! - records everything it did into history (including per-item failures)
//! - `NeedsDecision` ops never execute here; the caller resolves them first

use crate::history::{self, FailedItem, HistoryEntry, HistoryItem, HistoryKind};
use crate::types::{PlanStatus, PlannedOp};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ExecuteResult {
    pub entry: HistoryEntry,
    /// Ops that were skipped at execute time (e.g. unresolved NeedsDecision).
    pub skipped: Vec<PlannedOp>,
}

#[derive(Debug, Clone)]
pub enum AskResolution {
    /// Execute-time behaviour for ops still marked NeedsDecision.
    Skip,
}

/// Execute a plan, recording the outcome in `history_path`.
/// `op_counter` feeds history id generation; pass a shared counter.
pub fn execute(
    plan: &[PlannedOp],
    history_path: &Path,
    quarantine_dir: &Path,
    op_counter: &mut u64,
    ask: AskResolution,
) -> io::Result<ExecuteResult> {
    let mut items: Vec<HistoryItem> = Vec::new();
    let mut quarantined: Vec<HistoryItem> = Vec::new();
    let mut failed: Vec<FailedItem> = Vec::new();
    let mut skipped: Vec<PlannedOp> = Vec::new();

    for op in plan {
        match &op.status {
            PlanStatus::Move { replace } => match move_one(op, *replace, quarantine_dir) {
                Ok(stash) => {
                    items.push(HistoryItem {
                        src: op.src.clone(),
                        dst: op.dst.clone(),
                    });
                    if let Some(stash) = stash {
                        quarantined.push(HistoryItem {
                            src: op.dst.clone(),
                            dst: stash,
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!("move failed {}: {e}", op.src.display());
                    failed.push(FailedItem {
                        src: op.src.clone(),
                        error: e.to_string(),
                    });
                }
            },
            PlanStatus::Noop => {}
            PlanStatus::Skip { .. } => skipped.push(op.clone()),
            PlanStatus::NeedsDecision => match ask {
                AskResolution::Skip => skipped.push(op.clone()),
            },
        }
    }

    *op_counter += 1;
    let entry = HistoryEntry {
        id: history::new_op_id(*op_counter),
        ts: chrono::Utc::now(),
        kind: HistoryKind::Move,
        items,
        quarantined,
        failed,
    };
    if !entry.items.is_empty() || !entry.failed.is_empty() {
        history::append(history_path, &entry)?;
    }
    Ok(ExecuteResult { entry, skipped })
}

/// Reverse the successful items of a history entry, recording an undo entry.
/// Items unwind in reverse order; quarantined (replaced) files are restored
/// to their original destination after the incoming file is moved back.
pub fn undo(
    entry: &HistoryEntry,
    history_path: &Path,
    op_counter: &mut u64,
) -> io::Result<HistoryEntry> {
    let mut restored: Vec<HistoryItem> = Vec::new();
    let mut failed: Vec<FailedItem> = Vec::new();

    for item in entry.items.iter().rev() {
        // dst may not exist (user moved it away since); that's a soft failure.
        match move_raw(&item.dst, &item.src) {
            Ok(()) => restored.push(HistoryItem {
                src: item.dst.clone(),
                dst: item.src.clone(),
            }),
            Err(e) => failed.push(FailedItem {
                src: item.dst.clone(),
                error: format!("undo failed: {e}"),
            }),
        }
    }
    // Put back whatever Replace parked in quarantine.
    for item in entry.quarantined.iter().rev() {
        match move_raw(&item.dst, &item.src) {
            Ok(()) => restored.push(HistoryItem {
                src: item.dst.clone(),
                dst: item.src.clone(),
            }),
            Err(e) => failed.push(FailedItem {
                src: item.dst.clone(),
                error: format!("quarantine restore failed: {e}"),
            }),
        }
    }

    *op_counter += 1;
    let undo_entry = HistoryEntry {
        id: history::new_op_id(*op_counter),
        ts: chrono::Utc::now(),
        kind: HistoryKind::Undo,
        items: restored,
        quarantined: Vec::new(),
        failed,
    };
    history::append(history_path, &undo_entry)?;
    Ok(undo_entry)
}

/// Move one planned op. Returns the quarantine path when a Replace parked
/// the previous destination somewhere safe.
fn move_one(op: &PlannedOp, replace: bool, quarantine_dir: &Path) -> io::Result<Option<PathBuf>> {
    if let Some(parent) = op.dst.parent() {
        fs::create_dir_all(parent)?;
    }
    if replace && crate::fsutil::dest_exists(&op.dst)? {
        // Never destroy content: park the replaced file in the quarantine
        // area instead of deleting it. Undo moves it back.
        fs::create_dir_all(quarantine_dir)?;
        let stash = quarantine_dir.join(op.quarantine_name());
        move_raw(&op.dst, &stash)?;
        move_with_retry(&op.src, &op.dst)?;
        return Ok(Some(stash));
    }
    move_with_retry(&op.src, &op.dst)?;
    Ok(None)
}

impl PlannedOp {
    /// Unique file name for the quarantined copy of a replaced destination.
    fn quarantine_name(&self) -> String {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.src.hash(&mut h);
        self.dst.hash(&mut h);
        format!(
            "{:016x}_{}",
            h.finish(),
            self.src
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        )
    }
}

/// Move with locked-file retry: 250 ms, 500 ms, 1 s, 2 s, 4 s (~8 s total).
fn move_with_retry(src: &Path, dst: &Path) -> io::Result<()> {
    let delays = [250u64, 500, 1000, 2000, 4000];
    let mut last_err: Option<io::Error> = None;
    for (attempt, ms) in delays.iter().enumerate() {
        match move_raw(src, dst) {
            Ok(()) => return Ok(()),
            Err(e) if is_transient(&e) && attempt < delays.len() - 1 => {
                tracing::debug!("locked, retrying in {ms} ms: {}", src.display());
                sleep(Duration::from_millis(*ms));
                last_err = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.expect("retry loop returned without result"))
}

/// Errors worth retrying: sharing violations, access denied, transient
/// path-not-found (a parent directory mid-creation, a drive enumerating).
fn is_transient(e: &io::Error) -> bool {
    use io::ErrorKind::*;
    match e.kind() {
        PermissionDenied | NotFound => true,
        _ => {
            // Windows raw codes: 32 = SHARING_VIOLATION, 5 = ACCESS_DENIED.
            #[cfg(windows)]
            {
                const ERROR_SHARING_VIOLATION: i32 = 32;
                const ERROR_ACCESS_DENIED: i32 = 5;
                const ERROR_CLOUD_FILE_NOT_SUPPORTED: i32 = 395;
                if let Some(code) = e.raw_os_error() {
                    return matches!(code, ERROR_SHARING_VIOLATION | ERROR_ACCESS_DENIED)
                        || code == ERROR_CLOUD_FILE_NOT_SUPPORTED;
                }
            }
            false
        }
    }
}

/// Rename, falling back to copy+remove for cross-volume moves.
fn move_raw(src: &Path, dst: &Path) -> io::Result<()> {
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(rename_err) => {
            // EXDEV (unix) / "cannot move across volumes" (Windows): copy.
            if is_cross_volume(&rename_err) {
                copy_remove(src, dst)
            } else {
                Err(rename_err)
            }
        }
    }
}

fn is_cross_volume(e: &io::Error) -> bool {
    #[cfg(unix)]
    {
        return e.raw_os_error() == Some(libc_EXDEV);
    }
    #[cfg(windows)]
    {
        // ERROR_NOT_SAME_DEVICE = 17
        e.raw_os_error() == Some(17)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = e;
        false
    }
}

#[cfg(unix)]
const libc_EXDEV: i32 = 18;

fn copy_remove(src: &Path, dst: &Path) -> io::Result<()> {
    if src.is_dir() {
        // The planner only moves files/symlinks; a directory here means a
        // race with another process — refuse rather than recurse blindly.
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to copy a directory across volumes",
        ));
    }
    fs::copy(src, dst)?;
    fs::remove_file(src)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::plan_inputs;
    use crate::types::{PlanMode, Settings};

    fn settings_with_super(sf: &Path) -> Settings {
        Settings {
            super_folder: Some(sf.to_path_buf()),
            ..Settings::default()
        }
    }

    #[test]
    fn execute_moves_and_records_history() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("Super Folder");
        fs::create_dir_all(&sf).unwrap();
        let pdf = tmp.path().join("report.pdf");
        fs::write(&pdf, b"data").unwrap();

        let settings = settings_with_super(&sf);
        let plan = plan_inputs(
            std::slice::from_ref(&pdf),
            &[],
            &settings,
            PlanMode::SuperFolder,
        );
        let mut counter = 0u64;
        let result = execute(
            &plan,
            &tmp.path().join("history.jsonl"),
            &tmp.path().join("quarantine"),
            &mut counter,
            AskResolution::Skip,
        )
        .unwrap();

        assert!(sf.join("Documents").join("report.pdf").exists());
        assert!(!pdf.exists());
        assert_eq!(result.entry.items.len(), 1);
        assert!(result.entry.failed.is_empty());

        // History on disk round-trips.
        let entries = history::read(&tmp.path().join("history.jsonl"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, HistoryKind::Move);
    }

    #[test]
    fn undo_restores_original_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("Super Folder");
        fs::create_dir_all(&sf).unwrap();
        let pdf = tmp.path().join("report.pdf");
        fs::write(&pdf, b"data").unwrap();
        let jpg = tmp.path().join("photo.jpg");
        fs::write(&jpg, b"img").unwrap();

        let settings = settings_with_super(&sf);
        let inputs = vec![pdf.clone(), jpg.clone()];
        let plan = plan_inputs(&inputs, &[], &settings, PlanMode::SuperFolder);
        let mut counter = 0u64;
        let hist = tmp.path().join("history.jsonl");
        let result = execute(
            &plan,
            &hist,
            &tmp.path().join("quarantine"),
            &mut counter,
            AskResolution::Skip,
        )
        .unwrap();

        let undo_entry = undo(&result.entry, &hist, &mut counter).unwrap();
        assert!(pdf.exists());
        assert!(jpg.exists());
        assert!(!sf.join("Documents").join("report.pdf").exists());
        assert_eq!(undo_entry.items.len(), 2);
        assert_eq!(undo_entry.kind, HistoryKind::Undo);
    }

    #[test]
    fn replace_quarantines_instead_of_deleting() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("Super Folder");
        let docs = sf.join("Documents");
        fs::create_dir_all(&docs).unwrap();
        let existing = docs.join("report.pdf");
        fs::write(&existing, b"OLD").unwrap();
        let incoming = tmp.path().join("report.pdf");
        fs::write(&incoming, b"NEW").unwrap();

        let settings = Settings {
            conflict_policy: crate::types::ConflictPolicy::Replace,
            ..settings_with_super(&sf)
        };
        let plan = plan_inputs(
            std::slice::from_ref(&incoming),
            &[],
            &settings,
            PlanMode::SuperFolder,
        );
        assert!(matches!(plan[0].status, PlanStatus::Move { replace: true }));

        let mut counter = 0u64;
        let quarantine = tmp.path().join("quarantine");
        let result = execute(
            &plan,
            &tmp.path().join("history.jsonl"),
            &quarantine,
            &mut counter,
            AskResolution::Skip,
        )
        .unwrap();

        assert_eq!(fs::read(&existing).unwrap(), b"NEW");
        assert!(!incoming.exists());
        // The old content survives in quarantine.
        let stashed: Vec<_> = fs::read_dir(&quarantine).unwrap().flatten().collect();
        assert_eq!(stashed.len(), 1);
        assert_eq!(fs::read(stashed[0].path()).unwrap(), b"OLD");
        assert!(result.entry.failed.is_empty());
    }

    #[test]
    fn unresolved_asks_are_skipped_not_moved() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("Super Folder");
        let docs = sf.join("Documents");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("a.pdf"), b"old").unwrap();
        let incoming = tmp.path().join("a.pdf");
        fs::write(&incoming, b"new").unwrap();

        let settings = Settings {
            conflict_policy: crate::types::ConflictPolicy::Ask,
            ..settings_with_super(&sf)
        };
        let plan = plan_inputs(
            std::slice::from_ref(&incoming),
            &[],
            &settings,
            PlanMode::SuperFolder,
        );
        let mut counter = 0u64;
        let result = execute(
            &plan,
            &tmp.path().join("history.jsonl"),
            &tmp.path().join("q"),
            &mut counter,
            AskResolution::Skip,
        )
        .unwrap();

        assert!(incoming.exists(), "asked file must stay put");
        assert_eq!(result.skipped.len(), 1);
        assert!(result.entry.items.is_empty());
    }
}
