//! Drives for the browser's sidebar and free space for its status line.
//! Windows: drive letters. macOS: / plus /Volumes. Linux: / plus mounted
//! media under /media, /run/media and /mnt.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub struct Drive {
    pub name: String,
    pub path: PathBuf,
    pub total: u64,
    pub free: u64,
    /// USB sticks, SD cards and network drives (shown with another icon).
    pub removable: bool,
}

/// (total, free for this user) of the drive holding `path`.
pub fn space(path: &Path) -> Option<(u64, u64)> {
    imp::space(path)
}

pub fn list() -> Vec<Drive> {
    imp::list()
}

#[cfg(windows)]
mod imp {
    use super::Drive;
    use std::path::{Path, PathBuf};
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
    };

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;

    pub fn space(path: &Path) -> Option<(u64, u64)> {
        let (mut free, mut total) = (0u64, 0u64);
        // SAFETY: plain out-pointers to locals that outlive the call.
        unsafe {
            GetDiskFreeSpaceExW(
                &HSTRING::from(path.as_os_str()),
                Some(&mut free),
                Some(&mut total),
                None,
            )
        }
        .ok()?;
        Some((total, free))
    }

    pub fn list() -> Vec<Drive> {
        let mut buf = [0u16; 512];
        // SAFETY: the buffer is ours; the call writes at most its length.
        let len = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) } as usize;
        let text = String::from_utf16_lossy(&buf[..len.min(buf.len())]);
        let mut out = Vec::new();
        for root in text.split('\0').filter(|r| !r.is_empty()) {
            let wide = HSTRING::from(root);
            // SAFETY: a valid, NUL-terminated root path.
            let kind = unsafe { GetDriveTypeW(&wide) };
            if !matches!(kind, DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_REMOTE) {
                continue; // CD drives, RAM disks, unknown
            }
            let path = PathBuf::from(root);
            // An empty card reader has no space: leave it out.
            let Some((total, free)) = space(&path) else {
                continue;
            };
            let mut label = [0u16; 261];
            // SAFETY: the buffer is ours; unused out-params are None.
            let named =
                unsafe { GetVolumeInformationW(&wide, Some(&mut label), None, None, None, None) }
                    .is_ok();
            let label = if named {
                let end = label.iter().position(|&c| c == 0).unwrap_or(label.len());
                String::from_utf16_lossy(&label[..end])
            } else {
                String::new()
            };
            let letter = root.trim_end_matches('\\');
            let name = if label.is_empty() {
                if kind == DRIVE_FIXED {
                    format!("Local Disk ({letter})")
                } else {
                    format!("Drive ({letter})")
                }
            } else {
                format!("{label} ({letter})")
            };
            out.push(Drive {
                name,
                path,
                total,
                free,
                removable: kind != DRIVE_FIXED,
            });
        }
        out
    }
}

#[cfg(unix)]
mod imp {
    use super::Drive;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    // statvfs field types differ per platform (u32 / u64 / i64), hence the
    // casts, which are no-ops on some targets.
    #[allow(clippy::unnecessary_cast)]
    pub fn space(path: &Path) -> Option<(u64, u64)> {
        let c = CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: a valid C string and a zeroed struct for the result.
        if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
            return None;
        }
        let unit = st.f_frsize as u64;
        Some((st.f_blocks as u64 * unit, st.f_bavail as u64 * unit))
    }

    fn system_name() -> String {
        if cfg!(target_os = "macos") {
            "Macintosh HD".into()
        } else {
            "Computer".into()
        }
    }

    fn mounts_under(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn list() -> Vec<Drive> {
        let mut out = Vec::new();
        if let Some((total, free)) = space(Path::new("/")) {
            out.push(Drive {
                name: system_name(),
                path: PathBuf::from("/"),
                total,
                free,
                removable: false,
            });
        }
        let mut extra = Vec::new();
        if cfg!(target_os = "macos") {
            extra.extend(mounts_under(Path::new("/Volumes")));
        } else {
            let user = std::env::var("USER").unwrap_or_default();
            if !user.is_empty() {
                extra.extend(mounts_under(&Path::new("/media").join(&user)));
                extra.extend(mounts_under(&Path::new("/run/media").join(&user)));
            }
            extra.extend(mounts_under(Path::new("/mnt")));
        }
        for path in extra {
            // /Volumes also lists the system disk itself as a link to "/".
            if std::fs::canonicalize(&path).is_ok_and(|p| p == Path::new("/")) {
                continue;
            }
            let Some((total, free)) = space(&path) else {
                continue;
            };
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push(Drive {
                name,
                path,
                total,
                free,
                removable: true,
            });
        }
        out
    }
}
