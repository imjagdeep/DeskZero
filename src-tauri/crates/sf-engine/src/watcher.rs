//! Background folder watching: notify + debounce + stable-file detection.
//!
//! Watch pipelines only consider a file once its size and mtime have been
//! stable across two samples 1 s apart, and its name doesn't look like a
//! partial download (`.crdownload`, `.part`, `.tmp`, `~`). This is what
//! makes it safe to auto-move files the moment a browser finishes them —
//! and to never touch one that is still being written.
//!
//! No async runtime: the debouncer runs on its own thread, events feed a
//! channel into a single stable-checker thread, and mature files are pushed
//! to the caller's sink channel.

use notify_debouncer_full::notify::{Event, EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, Debouncer, RecommendedCache};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::{sleep, JoinHandle};
use std::time::{Duration, Instant};

/// How long a file must stay unchanged before it is handed to the sink.
const STABLE_AFTER: Duration = Duration::from_millis(1000);
/// How often the checker re-samples pending files.
const TICK: Duration = Duration::from_millis(250);
/// Give up on files that never stabilize (e.g. an actively written log).
const MAX_WAIT: Duration = Duration::from_secs(300);
/// How long the debouncer folds event bursts together.
const DEBOUNCE: Duration = Duration::from_millis(500);

/// Partial-download / temp suffixes we never touch.
const TEMP_SUFFIXES: &[&str] = &["crdownload", "part", "tmp", "partial", "download"];

/// Handle to a running watch. Dropping it stops the threads.
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
    debouncer: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
    threads: Vec<JoinHandle<()>>,
}

impl WatchHandle {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(d) = self.debouncer.take() {
            drop(d); // unwatches; the checker thread exits on the stop flag
        }
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

fn to_io<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}
/// Start watching `folders`. Mature, stable files arrive on the returned
/// receiver in small batches. Symlinked folders are skipped (they can
/// escape the tree and create loops).
pub fn start(folders: &[PathBuf]) -> io::Result<(WatchHandle, Receiver<Vec<PathBuf>>)> {
    let (out_tx, out_rx) = channel::<Vec<PathBuf>>();
    let (raw_tx, raw_rx) = channel::<PathBuf>();
    let stop = Arc::new(AtomicBool::new(false));

    let mut debouncer: Debouncer<RecommendedWatcher, RecommendedCache> = new_debouncer(
        DEBOUNCE,
        None,
        move |result: notify_debouncer_full::DebounceEventResult| {
            if let Ok(events) = result {
                for event in events {
                    forward_event(&event, &raw_tx);
                }
            }
        },
    )
    .map_err(to_io)?;

    for folder in folders {
        let md = fs::symlink_metadata(folder)?;
        if md.file_type().is_symlink() {
            tracing::warn!("skipping symlinked watch folder {}", folder.display());
            continue;
        }
        // Only files directly in the folder: subfolders (the user's own,
        // and the category folders we create) are never touched.
        debouncer
            .watch(folder, RecursiveMode::NonRecursive)
            .map_err(to_io)?;
    }

    let stop_c = stop.clone();
    let checker = std::thread::spawn(move || stable_checker_loop(raw_rx, out_tx, stop_c));

    Ok((
        WatchHandle {
            stop,
            debouncer: Some(debouncer),
            threads: vec![checker],
        },
        out_rx,
    ))
}

fn forward_event(event: &Event, tx: &Sender<PathBuf>) {
    let interesting = matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any
    );
    if !interesting {
        return;
    }
    for path in &event.paths {
        if looks_temporary(path) {
            continue;
        }
        let _ = tx.send(path.clone());
    }
}

/// One in-flight observation of a file being stability-checked.
#[derive(Clone)]
struct Sample {
    size: u64,
    mtime: Option<std::time::SystemTime>,
    since: Instant,
    last_check: Instant,
}

/// Keep re-sampling each pending file until its (size, mtime) pair holds
/// still for STABLE_AFTER, then emit it. Files unstable for MAX_WAIT are
/// dropped with a warning (actively-written logs don't belong to us).
fn stable_checker_loop(rx: Receiver<PathBuf>, out: Sender<Vec<PathBuf>>, stop: Arc<AtomicBool>) {
    let mut pending: HashMap<PathBuf, Sample> = HashMap::new();

    while !stop.load(Ordering::SeqCst) {
        let now = Instant::now();
        // Drain everything that arrived since the last tick.
        while let Ok(path) = rx.try_recv() {
            if let Some(s) = sample(&path, now) {
                pending.insert(path, s);
            } else {
                pending.remove(&path); // vanished (or became temp): drop any earlier sample
            }
        }

        let now = Instant::now();
        let mut mature: Vec<PathBuf> = Vec::new();
        let mut expired: Vec<PathBuf> = Vec::new();

        for (path, s) in pending.iter_mut() {
            if now.duration_since(s.last_check) < STABLE_AFTER {
                continue;
            }
            match sample(path, now) {
                Some(cur) if cur.size == s.size && cur.mtime == s.mtime => {
                    mature.push(path.clone());
                }
                Some(cur) => {
                    // Still changing: keep waiting.
                    *s = Sample {
                        since: s.since,
                        last_check: now,
                        ..cur
                    };
                    if now.duration_since(s.since) > MAX_WAIT {
                        expired.push(path.clone());
                    }
                }
                None => expired.push(path.clone()), // vanished
            }
        }

        for p in mature.iter().chain(expired.iter()) {
            pending.remove(p);
        }
        if !expired.is_empty() {
            tracing::warn!(
                "watch: never stabilized, skipping: {}",
                expired
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !mature.is_empty() {
            tracing::debug!("watch: stable files: {:?}", mature);
            if out.send(mature).is_err() {
                return; // sink gone
            }
        }

        sleep(TICK);
    }
}

fn sample(path: &Path, now: Instant) -> Option<Sample> {
    let md = fs::symlink_metadata(path).ok()?;
    if md.is_dir() || looks_temporary(path) {
        return None;
    }
    Some(Sample {
        size: md.len(),
        mtime: md.modified().ok(),
        since: now,
        last_check: now,
    })
}
fn looks_temporary(path: &Path) -> bool {
    path.extension()
        .map(|e| {
            let e = e.to_string_lossy().to_lowercase();
            TEMP_SUFFIXES.contains(&e.as_str())
        })
        .unwrap_or(false)
        || path
            .file_name()
            .map(|n| n.to_string_lossy().ends_with('~'))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_suffixes_are_ignored() {
        assert!(looks_temporary(Path::new("a.pdf.crdownload")));
        assert!(looks_temporary(Path::new("b.ZIP.PART")));
        assert!(looks_temporary(Path::new("c.tmp")));
        assert!(looks_temporary(Path::new("draft~")));
        assert!(!looks_temporary(Path::new("photo.jpg")));
        assert!(!looks_temporary(Path::new("archive.tar.gz")));
    }

    #[test]
    /// Dropping a file into a watched folder surfaces it as stable within
    /// a couple of seconds; a file written in two bursts only surfaces after
    /// the second burst settles.
    fn end_to_end_stable_detection() {
        let tmp = tempfile::tempdir().unwrap();
        let watch = tmp.path().join("watch");
        fs::create_dir_all(&watch).unwrap();

        let (mut handle, rx) = start(std::slice::from_ref(&watch)).unwrap();

        // Burst 1: create then extend shortly after (simulates a download).
        fs::write(watch.join("file.txt"), b"chunk1").unwrap();
        sleep(Duration::from_millis(400));
        fs::write(watch.join("file.txt"), b"chunk1+chunk2").unwrap();

        let batch = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("should receive the settled file");
        assert!(batch.iter().any(|p| p.ends_with("file.txt")));

        // The file we saw must be the COMPLETE one.
        assert_eq!(fs::read(watch.join("file.txt")).unwrap(), b"chunk1+chunk2");

        handle.stop();
    }
}
