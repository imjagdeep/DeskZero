//! Rename templates: deterministic, preview-first file renaming.
//!
//! Supported tokens:
//!   {date}           modification date, YYYY-MM-DD
//!   {time}           modification time, HH-MM-SS
//!   {original_name}  full original file name (stem + extension)
//!   {original_stem}  name without the extension
//!   {ext}            extension without the dot (may be empty)
//!   {counter}        1-based position, zero-padded to 3 (001, 002, …)
//!
//! Everything else is literal text. The template must produce a name;
//! applying is a separate step so the UI can always preview first.

use crate::fsutil::{dest_exists, paths_equal};
use crate::plan::file_meta;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct RenameOp {
    pub src: PathBuf,
    pub new_name: String,
    /// Final target (conflict-renamed if needed) — filled by `plan_rename`.
    pub dst: PathBuf,
    pub skipped: bool,
    pub reason: Option<String>,
}

pub const KNOWN_TOKENS: &[&str] = &[
    "date",
    "time",
    "original_name",
    "original_stem",
    "ext",
    "counter",
];

/// Render one file name from the template. `counter` is 1-based.
pub fn render_template(template: &str, path: &Path, counter: usize) -> io::Result<String> {
    let meta = file_meta(path)?;
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();

    let modified: chrono::DateTime<chrono::Utc> = meta.modified;
    let mut out = template.to_string();
    out = out.replace("{date}", &modified.format("%Y-%m-%d").to_string());
    out = out.replace("{time}", &modified.format("%H-%M-%S").to_string());
    out = out.replace("{original_name}", &file_name);
    out = out.replace("{original_stem}", &stem);
    out = out.replace("{ext}", &ext);
    out = out.replace("{counter}", &format!("{counter:03}"));

    // Anything that still looks like an unknown token is a typo the user
    // should see rather than a silent literal: keep it as-is (visible in
    // preview), but validate catches obviously broken templates.
    Ok(out)
}

/// Validate a template before planning: must be non-empty, must not produce
/// a path separator, and must contain at least one token (a pure literal
/// rename would collide with every file).
pub fn validate_template(template: &str) -> Result<(), String> {
    if template.trim().is_empty() {
        return Err("template is empty".into());
    }
    if template.contains('/') || template.contains('\\') {
        return Err("template must not contain path separators".into());
    }
    if template.contains("..") {
        return Err("template must not contain '..'".into());
    }
    if !KNOWN_TOKENS
        .iter()
        .any(|t| template.contains(&format!("{{{t}}}")))
    {
        return Err(format!(
            "template has no tokens; use one of: {}",
            KNOWN_TOKENS.join(", ")
        ));
    }
    Ok(())
}

/// Plan renames for a set of paths. Never touches disk: `dst` is computed
/// (including conflict renames) so the preview shows the final state.
/// The same new name for two different sources gets a conflict-renamed
/// second target, matching the mover's behaviour.
pub fn plan_rename(paths: &[PathBuf], template: &str) -> io::Result<Vec<RenameOp>> {
    validate_template(template).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let mut ops = Vec::new();
    // Targets already handed out in this batch: disk checks alone would give
    // two sources the same target and the second rename would overwrite.
    let mut claimed: Vec<PathBuf> = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        if !path.is_file() && !is_symlink(path) {
            ops.push(RenameOp {
                src: path.clone(),
                new_name: String::new(),
                dst: path.clone(),
                skipped: true,
                reason: Some("not a file".into()),
            });
            continue;
        }
        let new_name = render_template(template, path, i + 1)?;
        if new_name.is_empty() {
            ops.push(RenameOp {
                src: path.clone(),
                new_name,
                dst: path.clone(),
                skipped: true,
                reason: Some("template produced an empty name".into()),
            });
            continue;
        }
        let dst = path.with_file_name(&new_name);
        // Renaming to its own name (or only changing case) is not a conflict.
        let dst = if paths_equal(&dst, path) {
            dst
        } else {
            free_target(&dst, &claimed)?
        };
        claimed.push(dst.clone());
        ops.push(RenameOp {
            src: path.clone(),
            new_name,
            dst,
            skipped: false,
            reason: None,
        });
    }
    Ok(ops)
}

/// Apply planned renames. Best-effort per file: failures are collected, not
/// fatal to the batch. Returns (renamed, failed).
pub fn apply_rename(ops: &[RenameOp]) -> (usize, Vec<(PathBuf, String)>) {
    let mut renamed = 0;
    let mut failed = Vec::new();
    for op in ops.iter().filter(|o| !o.skipped) {
        if op.src == op.dst {
            renamed += 1; // already the desired name
            continue;
        }
        match fs::rename(&op.src, &op.dst) {
            Ok(()) => renamed += 1,
            Err(e) => failed.push((op.src.clone(), e.to_string())),
        }
    }
    (renamed, failed)
}

/// First free name for `path`, treating existing files and targets claimed
/// earlier in this batch as taken. Same "name (n).ext" scheme as
/// `fsutil::probe_free_name`.
fn free_target(path: &Path, claimed: &[PathBuf]) -> io::Result<PathBuf> {
    let taken = |p: &Path| -> io::Result<bool> {
        Ok(dest_exists(p)? || claimed.iter().any(|c| paths_equal(c, p)))
    };
    if !taken(path)? {
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
        if !taken(&candidate)? {
            return Ok(candidate);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("could not find a free name for {}", path.display()),
    ))
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn date_original_name_template() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("IMG_1234.jpg");
        fs::write(&p, b"x").unwrap();
        let name = render_template("{date}_{original_name}", &p, 1).unwrap();
        let today = chrono::Utc::now().format("%Y-%m-%d");
        assert_eq!(name, format!("{today}_IMG_1234.jpg"));
    }

    #[test]
    fn counter_is_zero_padded_and_one_based() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.txt");
        fs::write(&p, b"x").unwrap();
        assert_eq!(render_template("{counter}", &p, 1).unwrap(), "001");
        assert_eq!(render_template("{counter}", &p, 42).unwrap(), "042");
    }

    #[test]
    fn stem_and_ext_split_correctly() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("report.final.pdf");
        fs::write(&p, b"x").unwrap();
        assert_eq!(
            render_template("{original_stem}.{ext}", &p, 1).unwrap(),
            "report.final.pdf"
        );
    }

    #[test]
    fn unknown_tokens_survive_for_preview_visibility() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.txt");
        fs::write(&p, b"x").unwrap();
        assert!(render_template("{year}_{original_name}", &p, 1)
            .unwrap()
            .starts_with("{year}_"));
    }

    #[test]
    fn validation_rejects_tokenless_templates() {
        assert!(validate_template("photo").is_err());
        assert!(validate_template("").is_err());
        assert!(validate_template("a/b{date}").is_err());
        assert!(validate_template("{date}_{original_name}").is_ok());
    }

    #[test]
    fn plan_preview_never_touches_disk_and_resolves_conflicts() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.txt");
        let b = tmp.path().join("b.txt");
        fs::write(&a, b"1").unwrap();
        fs::write(&b, b"2").unwrap();

        // Both render to the same name: second must be conflict-renamed.
        let ops = plan_rename(&[a.clone(), b.clone()], "{ext}").unwrap();
        assert_eq!(ops[0].dst, tmp.path().join("txt"));
        assert_eq!(ops[1].dst, tmp.path().join("txt (1)"));
        assert!(a.exists() && b.exists(), "planning must not rename");
    }

    #[test]
    fn renaming_to_own_name_is_a_no_op() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.txt");
        fs::write(&a, b"1").unwrap();
        let ops = plan_rename(std::slice::from_ref(&a), "{original_name}").unwrap();
        assert_eq!(ops[0].dst, a);
        let (n, failed) = apply_rename(&ops);
        assert_eq!((n, failed.len()), (1, 0));
        assert!(a.exists());
    }

    #[test]
    fn apply_renames_then_reports() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("x.txt");
        fs::write(&a, b"1").unwrap();
        let ops = plan_rename(std::slice::from_ref(&a), "renamed-{original_name}").unwrap();
        let (n, failed) = apply_rename(&ops);
        assert_eq!(n, 1);
        assert!(failed.is_empty());
        assert!(tmp.path().join("renamed-x.txt").exists());
    }
}
