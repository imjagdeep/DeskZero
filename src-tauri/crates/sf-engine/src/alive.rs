//! When DeskZero was last running, so files that land in a watch folder
//! while it is closed (restart, update, reboot) are still picked up at the
//! next start. The app writes a heartbeat every minute; at start it reads the
//! previous one and treats files that arrived after it as new.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const ALIVE_FILE: &str = "alive.json";

#[derive(Serialize, Deserialize)]
struct Alive {
    /// Unix seconds.
    last_alive: u64,
}

/// The last heartbeat, or `None` on the first run (nothing to catch up).
pub fn read(data_dir: &Path) -> Option<SystemTime> {
    let text = fs::read_to_string(data_dir.join(ALIVE_FILE)).ok()?;
    let a: Alive = serde_json::from_str(&text).ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(a.last_alive))
}

/// Record "running now". Write-then-rename so a crash never leaves half a file.
pub fn write(data_dir: &Path) -> io::Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let text = serde_json::to_string(&Alive { last_alive: now })
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = data_dir.join("alive.json.tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, data_dir.join(ALIVE_FILE))
}

/// Loose files directly in `folder` that were created or changed after
/// `since`. Sub-folders and links are not included; the planner applies the
/// usual rules (hidden, system and ignored files are left alone).
pub fn arrived_since(folder: &Path, since: SystemTime) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(folder) else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|e| {
            let md = fs::symlink_metadata(e.path()).ok()?;
            if !md.is_file() {
                return None;
            }
            // A download keeps its original "modified" time in some
            // browsers, so the newer of created/modified decides.
            let newest = [md.created().ok(), md.modified().ok()]
                .into_iter()
                .flatten()
                .max()?;
            (newest > since).then(|| e.path())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read(dir.path()).is_none(), "first run has no heartbeat");
        write(dir.path()).unwrap();
        let t = read(dir.path()).unwrap();
        let age = SystemTime::now().duration_since(t).unwrap();
        assert!(age < Duration::from_secs(5));
    }

    #[test]
    fn only_files_that_arrived_after_count() {
        let dir = tempfile::tempdir().unwrap();
        let since = SystemTime::now() - Duration::from_secs(60);
        fs::write(dir.path().join("new.pdf"), b"x").unwrap();
        fs::create_dir(dir.path().join("Sub")).unwrap();
        fs::write(dir.path().join("Sub").join("inner.txt"), b"x").unwrap();

        let got = arrived_since(dir.path(), since);
        assert_eq!(
            got,
            vec![dir.path().join("new.pdf")],
            "sub-folders are not included"
        );

        // Nothing arrived after "now".
        let later = SystemTime::now() + Duration::from_secs(60);
        assert!(arrived_since(dir.path(), later).is_empty());
    }

    #[test]
    fn missing_folder_is_empty() {
        assert!(arrived_since(Path::new("Z:/no/such/folder"), UNIX_EPOCH).is_empty());
    }
}
