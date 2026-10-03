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
    Ok(FileMeta {
        path: path.to_path_buf(),
        name_lower: name.to_lowercase(),
        ext_lower: classify::extension_of(&name),
        category: classify::classify(&name),
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
    })
}

/// Plan a set of inputs. `inputs` may be files or directories; directories
/// are planned recursively (only their contents move, the directory itself
/// stays). Never moves anything.
///
/// `root_hint` distinguishes Super Folder drops from watch-folder events;
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
    let meta = match file_meta(path) {
        Ok(m) => m,
        Err(e) => {
            out.push(skipped(
                path,
                path,
                format!("cannot read metadata: {e}"),
                0,
            ));
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

fn plan_file(
    meta: &FileMeta,
    rules: &[Rule],
    settings: &Settings,
    mode: PlanMode,
) -> PlannedOp {
    let src = meta.path.clone();

    let (rule_id, rule_name, destination) = match rules::resolve_destination(rules, meta) {
        Some((rule, dest)) => (
            Some(rule.id.clone()),
            Some(rule.name.clone()),
            Some(dest),
        ),
        None => match mode {
            // Super Folder drop with no matching rule → its classified
            // category (pdf → Documents, unknown → Other).
            PlanMode::SuperFolder => (None, None, Some(Destination::Category {
                category: meta.category,
            })),
            // Watch folder with no matching rule → leave in place, flag it.
            PlanMode::WatchFolder => {
                return skipped(&src, &src, "no matching rule".into(), meta.size_bytes);
            }
        },
    };
    let Some(destination) = destination else {
        return skipped(&src, &src, "no destination".into(), meta.size_bytes);
    };

    let Some(dst) = compute_destination(&destination, &src, settings) else {
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
        Err(e) => skipped(&src, &dst, format!("cannot check destination: {e}"), meta.size_bytes),
    }
}

/// Map a rule destination to a concrete target path for `src`.
fn compute_destination(dest: &Destination, src: &Path, settings: &Settings) -> Option<PathBuf> {
    let file_name = src.file_name()?;
    match dest {
        Destination::Category { category } => {
            let root = settings.super_folder.as_ref()?;
            Some(root.join(category.folder_name()).join(file_name))
        }
        Destination::Custom { path } => {
            if path.is_absolute() {
                Some(path.join(file_name))
            } else {
                // Relative custom paths anchor at the Super Folder.
                let root = settings.super_folder.as_ref()?;
                Some(root.join(path).join(file_name))
            }
        }
    }
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
        Category, Condition, ConditionField, CondValue, Destination, Op, Rule, RuleKind,
    };

    fn settings_with_super(dir: &Path) -> Settings {
        Settings {
            super_folder: Some(dir.to_path_buf()),
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
        }
    }

    #[test]
    fn plans_moves_into_category_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("Super Folder");
        fs::create_dir_all(&sf).unwrap();
        let pdf = tmp.path().join("invoice.pdf");
        fs::write(&pdf, b"x").unwrap();

        let settings = settings_with_super(&sf);
        let plan = plan_inputs(&[pdf.clone()], &[], &settings, PlanMode::SuperFolder);

        assert_eq!(plan.len(), 1);
        let op = &plan[0];
        assert_eq!(op.src, pdf);
        assert_eq!(
            op.dst,
            sf.join("Documents").join("invoice.pdf")
        );
        assert!(matches!(op.status, PlanStatus::Move { replace: false }));
    }

    #[test]
    fn unmatched_watch_folder_file_stays_put() {
        let tmp = tempfile::tempdir().unwrap();
        let watch = tmp.path().join("Downloads");
        fs::create_dir_all(&watch).unwrap();
        let odd = watch.join("mystery.zzz");
        fs::write(&odd, b"x").unwrap();

        let settings = Settings::default(); // no super folder configured
        let plan = plan_inputs(&[odd.clone()], &[], &settings, PlanMode::WatchFolder);
        assert!(matches!(
            &plan[0].status,
            PlanStatus::Skip { reason } if reason == "no matching rule"
        ));
        assert_eq!(plan[0].dst, odd);
    }

    #[test]
    fn custom_rule_beats_default_category() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("Super Folder");
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
                destination: Destination::Custom { path: invoices.clone() },
            },
            ext_rule("pdf", "pdf", Category::Documents),
        ];
        let settings = settings_with_super(&sf);
        let plan = plan_inputs(&[pdf.clone()], &rules, &settings, PlanMode::SuperFolder);
        assert_eq!(plan[0].dst, invoices.join("july-invoice.pdf"));
        assert_eq!(plan[0].rule_id.as_deref(), Some("inv"));
    }

    #[test]
    fn conflict_renames_at_plan_time() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("Super Folder");
        let docs = sf.join("Documents");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("report.pdf"), b"old").unwrap();

        let incoming = tmp.path().join("report.pdf");
        fs::write(&incoming, b"new").unwrap();

        let settings = settings_with_super(&sf);
        let plan = plan_inputs(&[incoming.clone()], &[], &settings, PlanMode::SuperFolder);
        assert_eq!(plan[0].dst.file_name().unwrap(), "report (1).pdf");
        assert!(matches!(plan[0].status, PlanStatus::Move { replace: false }));
    }

    #[test]
    fn skip_policy_flags_conflicts() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("Super Folder");
        let docs = sf.join("Documents");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("report.pdf"), b"old").unwrap();
        let incoming = tmp.path().join("report.pdf");
        fs::write(&incoming, b"new").unwrap();

        let settings = Settings {
            conflict_policy: ConflictPolicy::Skip,
            ..settings_with_super(&sf)
        };
        let plan = plan_inputs(&[incoming.clone()], &[], &settings, PlanMode::SuperFolder);
        assert!(matches!(
            &plan[0].status,
            PlanStatus::Skip { reason } if reason == "already exists"
        ));
    }

    #[test]
    fn ask_policy_marks_undecided() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("Super Folder");
        let docs = sf.join("Documents");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("a.pdf"), b"old").unwrap();
        let incoming = tmp.path().join("a.pdf");
        fs::write(&incoming, b"new").unwrap();

        let settings = Settings {
            conflict_policy: ConflictPolicy::Ask,
            ..settings_with_super(&sf)
        };
        let plan = plan_inputs(&[incoming.clone()], &[], &settings, PlanMode::SuperFolder);
        assert!(matches!(plan[0].status, PlanStatus::NeedsDecision));
    }

    #[test]
    fn directories_are_traversed_not_moved() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("Super Folder");
        fs::create_dir_all(&sf).unwrap();
        let pile = tmp.path().join("pile");
        fs::create_dir_all(&pile).unwrap();
        fs::write(pile.join("a.jpg"), b"x").unwrap();
        fs::write(pile.join("b.pdf"), b"x").unwrap();

        let settings = settings_with_super(&sf);
        let plan = plan_inputs(&[pile.clone()], &[], &settings, PlanMode::SuperFolder);
        assert_eq!(plan.len(), 2);
        assert!(plan.iter().all(|op| op.src.parent() == Some(pile.as_path())));
        assert!(pile.exists(), "source directory must survive");
    }

    #[test]
    fn missing_file_is_flagged_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("Super Folder");
        fs::create_dir_all(&sf).unwrap();
        let ghost = tmp.path().join("ghost.pdf");
        let settings = settings_with_super(&sf);
        let plan = plan_inputs(&[ghost], &[], &settings, PlanMode::SuperFolder);
        assert!(matches!(&plan[0].status, PlanStatus::Skip { .. }));
    }

    #[test]
    fn noop_when_already_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let sf = tmp.path().join("Super Folder");
        let docs = sf.join("Documents");
        fs::create_dir_all(&docs).unwrap();
        let at_rest = docs.join("report.pdf");
        fs::write(&at_rest, b"x").unwrap();

        let settings = settings_with_super(&sf);
        let plan = plan_inputs(&[at_rest.clone()], &[], &settings, PlanMode::SuperFolder);
        assert!(matches!(plan[0].status, PlanStatus::Noop));
    }
}
