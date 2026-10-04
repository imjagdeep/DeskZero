//! The planner: turns a set of input paths into a `Vec<PlannedOp>` without
//! touching disk. Preview renders this; the mover executes it; undo reverses
//! what was recorded from it.
//!
//! For every input file the pipeline is:
//!   metadata → rule resolution → destination path → safety guards →
//!   conflict pre-check (rename probing happens HERE so the preview shows
//!   the final name) → PlannedOp

use crate::classify;
use crate::fsutil::{dest_exists, paths_equal, probe_free_name};
use crate::rules;
use crate::types::{
    ConflictPolicy, Destination, FileMeta, PlanMode, PlanStatus, PlannedOp, Rule, Settings,
};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Build metadata for one path without following symlinks.
pub fn file_meta(path: &Path) -> io::Result<FileMeta> {
    // symlink_metadata: we stat the link itself, never its target.
    let md = fs::symlink_metadata(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ft = md.file_type();
    let category = classify::classify(&name);
    let taken = if category == crate::types::Category::Images && ft.is_file() {
        photo_taken(path)
    } else {
        None
    };
    Ok(FileMeta {
        path: path.to_path_buf(),
        name_lower: name.to_lowercase(),
        ext_lower: classify::extension_of(&name),
        category,
        size_bytes: md.len(),
        created: md
            .created()
            .map(|t| t.into())
            .unwrap_or_else(|_| chrono::Utc::now()),
        modified: md
            .modified()
            .map(|t| t.into())
            .unwrap_or_else(|_| chrono::Utc::now()),
        is_symlink: ft.is_symlink(),
        is_dir: ft.is_dir(),
        taken,
    })
}

/// EXIF "date taken" of a photo, read from the file header only. Any read
/// or parse problem simply means "unknown" (falls back to modified date).
fn photo_taken(path: &Path) -> Option<chrono::NaiveDateTime> {
    let file = fs::File::open(path).ok()?;
    let mut reader = io::BufReader::new(file);
    let exif = exif::Reader::new().read_from_container(&mut reader).ok()?;
    let field = exif
        .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .or_else(|| exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY))?;
    let exif::Value::Ascii(ref parts) = field.value else {
        return None;
    };
    let raw = parts.first()?;
    let text = std::str::from_utf8(raw).ok()?.trim();
    // EXIF format: "YYYY:MM:DD HH:MM:SS"
    chrono::NaiveDateTime::parse_from_str(text, "%Y:%m:%d %H:%M:%S").ok()
}

/// Plan a set of inputs. `inputs` may be files or directories; directories
/// are planned recursively (only their contents move, the directory itself
/// stays). Never moves anything.
///
/// `root_hint` distinguishes DeskZero drops from watch-folder events;
/// it decides what happens when no rule matches.
pub fn plan_inputs(
    inputs: &[PathBuf],
    rules: &[Rule],
    settings: &Settings,
    mode: PlanMode,
) -> Vec<PlannedOp> {
    let mut out = Vec::new();
    for input in inputs {
        // Expand directories one level at a time via walkdir-free recursion
        // so symlinks inside are never followed.
        plan_one(input, rules, settings, mode, &mut out);
    }
    out
}

fn plan_one(
    path: &Path,
    rules: &[Rule],
    settings: &Settings,
    mode: PlanMode,
    out: &mut Vec<PlannedOp>,
) {
    // desktop.ini, Thumbs.db, .DS_Store, Office lock files and anything the
    // OS marks hidden/system belong where they are: never planned.
    if crate::fsutil::is_hidden_or_system(path)
        || crate::fsutil::is_ignored(path, &settings.ignore_patterns)
    {
        return;
    }
    let meta = match file_meta(path) {
        Ok(m) => m,
        Err(e) => {
            out.push(skipped(path, path, format!("cannot read metadata: {e}"), 0));
            return;
        }
    };

    if meta.is_dir && !meta.is_symlink {
        // Directories are traversed, not moved. This keeps the model simple
        // and avoids whole-tree moves into rule destinations.
        match fs::read_dir(path) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    plan_one(&entry.path(), rules, settings, mode, out);
                }
            }
            Err(e) => out.push(skipped(
                path,
                path,
                format!("cannot list directory: {e}"),
                meta.size_bytes,
            )),
        }
        return;
    }

    // Symlinks: plan to move the link itself, never the target.
    out.push(plan_file(&meta, rules, settings, mode));
}

fn plan_file(meta: &FileMeta, rules: &[Rule], settings: &Settings, mode: PlanMode) -> PlannedOp {
    let src = meta.path.clone();

    let mut rename: Option<String> = None;
    let (rule_id, rule_name, destination) = match rules::resolve_destination(rules, meta) {
        Some((rule, dest)) => {
            rename = rule.rename.clone().filter(|t| !t.trim().is_empty());
            (Some(rule.id.clone()), Some(rule.name.clone()), Some(dest))
        }
        None => match mode {
            // DeskZero drop with no matching rule → its classified
            // category (pdf → Documents, unknown → Other).
            PlanMode::OrganizeRoot => (
                None,
                None,
                Some(Destination::Category {
                    category: meta.category,
                }),
            ),
            // Watch folder with no matching rule → sort by its classified
            // type when the fallback is on, else leave in place and flag it.
            PlanMode::WatchFolder => {
                if settings.watch_type_fallback {
                    (
                        None,
                        None,
                        Some(Destination::Category {
                            category: meta.category,
                        }),
                    )
                } else {
                    return skipped(&src, &src, "no matching rule".into(), meta.size_bytes);
                }
            }
        },
    };
    let Some(destination) = destination else {
        return skipped(&src, &src, "no destination".into(), meta.size_bytes);
    };

    let Some(dst) = compute_destination(&destination, meta, rename.as_deref(), settings) else {
        return skipped(
            &src,
            &src,
            "invalid destination path".into(),
            meta.size_bytes,
        );
    };

    // Safety guards -------------------------------------------------------

    // Folder-into-itself: dst may not be equal to or inside src. For single
    // files src==dst is the common noop case, handled by paths_equal below;
    // the starts_with guard protects directory-ish sources and misconfigured
    // rules pointing back into the source folder.
    if let (Ok(src_c), Ok(dst_c)) = (fs::canonicalize(&src), fs::canonicalize(&dst)) {
        if dst_c.starts_with(&src_c) && !paths_equal(&src, &dst) {
            return skipped(
                &src,
                &dst,
                "destination is inside the source".into(),
                meta.size_bytes,
            );
        }
    }

    if paths_equal(&src, &dst) {
        return PlannedOp {
            src,
            dst,
            status: PlanStatus::Noop,
            rule_id,
            rule_name,
            size_bytes: meta.size_bytes,
        };
    }

    // Conflict pre-check ---------------------------------------------------

    match dest_exists(&dst) {
        Ok(true) => match settings.conflict_policy {
            ConflictPolicy::Rename => match probe_free_name(&dst) {
                Ok(free) => PlannedOp {
                    src,
                    dst: free,
                    status: PlanStatus::Move { replace: false },
                    rule_id,
                    rule_name,
                    size_bytes: meta.size_bytes,
                },
                Err(e) => skipped(&src, &dst, format!("{e}"), meta.size_bytes),
            },
            ConflictPolicy::Skip => skipped(&src, &dst, "already exists".into(), meta.size_bytes),
            ConflictPolicy::Replace => PlannedOp {
                src,
                dst,
                status: PlanStatus::Move { replace: true },
                rule_id,
                rule_name,
                size_bytes: meta.size_bytes,
            },
            ConflictPolicy::Ask => PlannedOp {
                src,
                dst,
                status: PlanStatus::NeedsDecision,
                rule_id,
                rule_name,
                size_bytes: meta.size_bytes,
            },
        },
        Ok(false) => PlannedOp {
            src,
            dst,
            status: PlanStatus::Move { replace: false },
            rule_id,
            rule_name,
            size_bytes: meta.size_bytes,
        },
        Err(e) => skipped(
            &src,
            &dst,
            format!("cannot check destination: {e}"),
            meta.size_bytes,
        ),
    }
}

/// Map a rule destination to a concrete target path for a file.
/// Custom paths may use `{year}`, `{month}` and `{day}` (date taken for
/// photos, else modified date); `rename` is an optional renamer template
/// for the new file name.
fn compute_destination(
    dest: &Destination,
    meta: &FileMeta,
    rename: Option<&str>,
    settings: &Settings,
) -> Option<PathBuf> {
    let file_name: std::ffi::OsString = match rename {
        Some(template) => {
            let name = crate::renamer::render_template(template, &meta.path, 1).ok()?;
            let bad = name.trim().is_empty()
                || name.contains('/')
                || name.contains('\\')
                || name == "."
                || name == "..";
            if bad {
                return None;
            }
            name.into()
        }
        None => meta.path.file_name()?.to_os_string(),
    };
    match dest {
        Destination::Category { category } => {
            let root = settings.organize_root.as_ref()?;
            Some(root.join(category.folder_name()).join(file_name))
        }
        Destination::Custom { path } => {
            let path = expand_date_tokens(path, meta.sort_date());
            if path.is_absolute() {
                Some(path.join(file_name))
            } else {
                // Relative custom paths anchor at your DeskZero and must
                // stay inside it: `..`, roots and drive prefixes are refused
                // ("invalid destination path" in the preview).
                let inside = path.components().all(|c| {
                    matches!(
                        c,
                        std::path::Component::Normal(_) | std::path::Component::CurDir
                    )
                });
                if !inside {
                    return None;
                }
                let root = settings.organize_root.as_ref()?;
                // Rebuild from components so `a/b` uses the native separator.
                let rel: PathBuf = path.components().collect();
                Some(root.join(rel).join(file_name))
            }
        }
    }
}

/// `Images/{year}/{month}` → `Images/2026/10`.
fn expand_date_tokens(path: &Path, date: chrono::NaiveDate) -> PathBuf {
    let text = path.to_string_lossy();
    if !text.contains('{') {
        return path.to_path_buf();
    }
    PathBuf::from(
        text.replace("{year}", &date.format("%Y").to_string())
            .replace("{month}", &date.format("%m").to_string())
            .replace("{day}", &date.format("%d").to_string()),
    )
}

fn skipped(src: &Path, dst: &Path, reason: String, size: u64) -> PlannedOp {
    PlannedOp {
        src: src.to_path_buf(),
        dst: dst.to_path_buf(),
        status: PlanStatus::Skip { reason },
        rule_id: None,
        rule_name: None,
        size_bytes: size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        Category, CondValue, Condition, ConditionField, Destination, Op, Rule, RuleKind,
    };

    fn settings_with_super(dir: &Path) -> Settings {
        Settings {
            organize_root: Some(dir.to_path_buf()),
            ..Settings::default()
        }
    }

    fn ext_rule(id: &str, ext: &str, cat: Category) -> Rule {
        Rule {
            id: id.into(),
            name: id.into(),
            enabled: true,
            kind: RuleKind::Extension,
            conditions: vec![Condition {
                field: ConditionField::Extension,
                op: Op::Is,
                value: CondValue::Text(ext.into()),
            }],
            destination: Destination::Category { category: cat },
            rename: None,
        }
    }

    #[test]
    fn plans_moves_into_category_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("DeskZero");
        fs::create_dir_all(&sf).unwrap();
        let pdf = tmp.path().join("invoice.pdf");
        fs::write(&pdf, b"x").unwrap();

        let settings = settings_with_super(&sf);
        let plan = plan_inputs(
            std::slice::from_ref(&pdf),
            &[],
            &settings,
            PlanMode::OrganizeRoot,
        );

        assert_eq!(plan.len(), 1);
        let op = &plan[0];
        assert_eq!(op.src, pdf);
        assert_eq!(op.dst, sf.join("Documents").join("invoice.pdf"));
        assert!(matches!(op.status, PlanStatus::Move { replace: false }));
    }

    #[test]
    fn unmatched_watch_folder_file_stays_put() {
        let tmp = tempfile::tempdir().unwrap();
        let watch = tmp.path().join("Downloads");
        fs::create_dir_all(&watch).unwrap();
        let odd = watch.join("mystery.zzz");
        fs::write(&odd, b"x").unwrap();

        let settings = Settings::default(); // no organize root configured
        let plan = plan_inputs(
            std::slice::from_ref(&odd),
            &[],
            &settings,
            PlanMode::WatchFolder,
        );
        assert!(matches!(
            &plan[0].status,
            PlanStatus::Skip { reason } if reason == "no matching rule"
        ));
        assert_eq!(plan[0].dst, odd);
    }

    #[test]
    fn watch_type_fallback_files_by_category_into_deskzero() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("DeskZero");
        let watch = tmp.path().join("Downloads");
        fs::create_dir_all(&watch).unwrap();
        let png = watch.join("screenshot.png");
        fs::write(&png, b"x").unwrap();

        let settings = Settings {
            organize_root: Some(sf.clone()),
            watch_type_fallback: true,
            ..Settings::default()
        };
        let plan = plan_inputs(
            std::slice::from_ref(&png),
            &[], // no rules at all
            &settings,
            PlanMode::WatchFolder,
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].dst, sf.join("Images").join("screenshot.png"));
        assert!(matches!(plan[0].status, PlanStatus::Move { replace: false }));
    }

    #[test]
    fn custom_rule_beats_default_category() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("DeskZero");
        let invoices = sf.join("Documents").join("Invoices");
        fs::create_dir_all(&invoices).unwrap();
        let pdf = tmp.path().join("july-invoice.pdf");
        fs::write(&pdf, b"x").unwrap();

        let rules = vec![
            Rule {
                id: "inv".into(),
                name: "Invoices".into(),
                enabled: true,
                kind: RuleKind::Custom,
                conditions: vec![
                    Condition {
                        field: ConditionField::Extension,
                        op: Op::Is,
                        value: CondValue::Text("pdf".into()),
                    },
                    Condition {
                        field: ConditionField::Filename,
                        op: Op::Contains,
                        value: CondValue::Text("invoice".into()),
                    },
                ],
                destination: Destination::Custom {
                    path: invoices.clone(),
                },
                rename: None,
            },
            ext_rule("pdf", "pdf", Category::Documents),
        ];
        let settings = settings_with_super(&sf);
        let plan = plan_inputs(
            std::slice::from_ref(&pdf),
            &rules,
            &settings,
            PlanMode::OrganizeRoot,
        );
        assert_eq!(plan[0].dst, invoices.join("july-invoice.pdf"));
        assert_eq!(plan[0].rule_id.as_deref(), Some("inv"));
    }

    #[test]
    fn ignore_patterns_leave_files_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("SF");
        fs::create_dir_all(&sf).unwrap();
        let link = sf.join("Chrome.lnk");
        let doc = sf.join("notes.txt");
        fs::write(&link, b"x").unwrap();
        fs::write(&doc, b"x").unwrap();
        let mut s = settings_with_super(&sf);
        s.ignore_patterns = vec!["*.LNK".into()];
        let plan = plan_inputs(&[link, doc.clone()], &[], &s, PlanMode::OrganizeRoot);
        assert_eq!(plan.len(), 1, "ignored file must not appear at all");
        assert_eq!(plan[0].src, doc);
    }

    #[test]
    fn date_tokens_and_rename_shape_the_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("SF");
        fs::create_dir_all(&sf).unwrap();
        let src = sf.join("scan.pdf");
        fs::write(&src, b"x").unwrap();
        let mut rule = ext_rule("pdfs", "pdf", Category::Documents);
        rule.destination = Destination::Custom {
            path: PathBuf::from("Docs/{year}/{month}"),
        };
        rule.rename = Some("{date}_{original_name}".into());
        let plan = plan_inputs(
            std::slice::from_ref(&src),
            &[rule],
            &settings_with_super(&sf),
            PlanMode::OrganizeRoot,
        );
        let today = chrono::Local::now().date_naive();
        let expected = sf
            .join("Docs")
            .join(today.format("%Y").to_string())
            .join(today.format("%m").to_string())
            .join(format!("{}_scan.pdf", today.format("%Y-%m-%d")));
        assert_eq!(plan[0].dst, expected);
        assert!(matches!(plan[0].status, PlanStatus::Move { .. }));
    }

    #[test]
    fn relative_destination_cannot_escape_organize_root() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("SF");
        fs::create_dir_all(&sf).unwrap();
        let src = tmp.path().join("a.pdf");
        fs::write(&src, b"x").unwrap();
        let mut rule = ext_rule("evil", "pdf", Category::Documents);
        rule.destination = Destination::Custom {
            path: PathBuf::from("../outside"),
        };
        let plan = plan_inputs(
            &[src],
            &[rule],
            &settings_with_super(&sf),
            PlanMode::OrganizeRoot,
        );
        assert!(matches!(&plan[0].status, PlanStatus::Skip { .. }));
    }

    #[test]
    fn conflict_renames_at_plan_time() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("DeskZero");
        let docs = sf.join("Documents");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("report.pdf"), b"old").unwrap();

        let incoming = tmp.path().join("report.pdf");
        fs::write(&incoming, b"new").unwrap();

        let settings = settings_with_super(&sf);
        let plan = plan_inputs(
            std::slice::from_ref(&incoming),
            &[],
            &settings,
            PlanMode::OrganizeRoot,
        );
        assert_eq!(plan[0].dst.file_name().unwrap(), "report (1).pdf");
        assert!(matches!(
            plan[0].status,
            PlanStatus::Move { replace: false }
        ));
    }

    #[test]
    fn skip_policy_flags_conflicts() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("DeskZero");
        let docs = sf.join("Documents");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("report.pdf"), b"old").unwrap();
        let incoming = tmp.path().join("report.pdf");
        fs::write(&incoming, b"new").unwrap();

        let settings = Settings {
            conflict_policy: ConflictPolicy::Skip,
            ..settings_with_super(&sf)
        };
        let plan = plan_inputs(
            std::slice::from_ref(&incoming),
            &[],
            &settings,
            PlanMode::OrganizeRoot,
        );
        assert!(matches!(
            &plan[0].status,
            PlanStatus::Skip { reason } if reason == "already exists"
        ));
    }

    #[test]
    fn ask_policy_marks_undecided() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("DeskZero");
        let docs = sf.join("Documents");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("a.pdf"), b"old").unwrap();
        let incoming = tmp.path().join("a.pdf");
        fs::write(&incoming, b"new").unwrap();

        let settings = Settings {
            conflict_policy: ConflictPolicy::Ask,
            ..settings_with_super(&sf)
        };
        let plan = plan_inputs(
            std::slice::from_ref(&incoming),
            &[],
            &settings,
            PlanMode::OrganizeRoot,
        );
        assert!(matches!(plan[0].status, PlanStatus::NeedsDecision));
    }

    #[test]
    fn directories_are_traversed_not_moved() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("DeskZero");
        fs::create_dir_all(&sf).unwrap();
        let pile = tmp.path().join("pile");
        fs::create_dir_all(&pile).unwrap();
        fs::write(pile.join("a.jpg"), b"x").unwrap();
        fs::write(pile.join("b.pdf"), b"x").unwrap();

        let settings = settings_with_super(&sf);
        let plan = plan_inputs(
            std::slice::from_ref(&pile),
            &[],
            &settings,
            PlanMode::OrganizeRoot,
        );
        assert_eq!(plan.len(), 2);
        assert!(plan
            .iter()
            .all(|op| op.src.parent() == Some(pile.as_path())));
        assert!(pile.exists(), "source directory must survive");
    }

    #[test]
    fn missing_file_is_flagged_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("DeskZero");
        fs::create_dir_all(&sf).unwrap();
        let ghost = tmp.path().join("ghost.pdf");
        let settings = settings_with_super(&sf);
        let plan = plan_inputs(&[ghost], &[], &settings, PlanMode::OrganizeRoot);
        assert!(matches!(&plan[0].status, PlanStatus::Skip { .. }));
    }

    #[test]
    fn noop_when_already_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("DeskZero");
        let docs = sf.join("Documents");
        fs::create_dir_all(&docs).unwrap();
        let at_rest = docs.join("report.pdf");
        fs::write(&at_rest, b"x").unwrap();

        let settings = settings_with_super(&sf);
        let plan = plan_inputs(
            std::slice::from_ref(&at_rest),
            &[],
            &settings,
            PlanMode::OrganizeRoot,
        );
        assert!(matches!(plan[0].status, PlanStatus::Noop));
    }
}
