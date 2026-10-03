//! Rule matching: conditions against file metadata, then tiered resolution.
//!
//! Evaluation order (first match wins):
//!   1. filename rules (in user order)
//!   2. custom rules (in user order)
//!   3. extension rules (in user order)
//!   4. caller's fallback (default category or "leave in place")

use crate::types::{Condition, ConditionField, Destination, FileMeta, Op, Rule, RuleKind};
use chrono::{DateTime, TimeZone, Utc};

/// Does one condition hold for this file?
///
/// Semantics per field:
/// - `Extension`: `Is`/`In` compare the lowercased extension without dot;
///   `Contains` matches a substring of it.
/// - `Filename`: `Contains` is case-insensitive substring; `Is` is a
///   case-insensitive exact name match; `In` checks the list.
/// - `Type`: `Is`/`In` compare against `Category::rule_value()`.
/// - `SizeMb`: numeric comparisons against the file size in MB (MiB? no —
///   decimal MB, 1_000_000 bytes, so "1000 MB = 1 GB" reads naturally).
/// - `Created`/`Modified`: value is an ISO-8601 date; `Gt` = after that date,
///   `Lt` = before, `Is`/`Lte`+`Gte` = within the same calendar day.
///
/// A condition whose operator doesn't make sense for its field simply never
/// matches — invalid rules match nothing instead of matching everything.
pub fn condition_matches(cond: &Condition, meta: &FileMeta) -> bool {
    match cond.field {
        ConditionField::Extension => match cond.op {
            Op::Is => text_eq(cond, &meta.ext_lower),
            Op::In => list_contains(cond, &meta.ext_lower),
            Op::Contains => cond
                .value
                .as_text()
                .map(|s| meta.ext_lower.contains(&s.to_lowercase()))
                .unwrap_or(false),
            _ => false,
        },
        ConditionField::Filename => match cond.op {
            Op::Is => text_eq(cond, &meta.name_lower),
            Op::Contains => cond
                .value
                .as_text()
                .map(|s| meta.name_lower.contains(&s.to_lowercase()))
                .unwrap_or(false),
            Op::In => list_contains(cond, &meta.name_lower),
            _ => false,
        },
        ConditionField::Type => {
            let v = meta.category.rule_value();
            match cond.op {
                Op::Is => text_eq(cond, v),
                Op::In => list_contains(cond, v),
                _ => false,
            }
        }
        ConditionField::SizeMb => {
            let Some(n) = cond.value.as_num() else {
                return false;
            };
            let size_mb = meta.size_bytes as f64 / 1_000_000.0;
            match cond.op {
                Op::Is => (size_mb - n).abs() < f64::EPSILON,
                Op::Lt => size_mb < n,
                Op::Lte => size_mb <= n,
                Op::Gt => size_mb > n,
                Op::Gte => size_mb >= n,
                Op::Contains | Op::In => false,
            }
        }
        ConditionField::Created | ConditionField::Modified => {
            let Some(text) = cond.value.as_text() else {
                return false;
            };
            let Some(ts) = parse_rule_date(text) else {
                return false;
            };
            let file_ts = if cond.field == ConditionField::Created {
                meta.created
            } else {
                meta.modified
            };
            let day_start = ts.date_naive().and_hms_opt(0, 0, 0).unwrap();
            let next_day = day_start + chrono::Duration::days(1);
            let day_start = Utc.from_utc_datetime(&day_start);
            let next_day = Utc.from_utc_datetime(&next_day);
            match cond.op {
                Op::Gt => file_ts >= next_day,
                Op::Gte => file_ts >= day_start,
                Op::Lt => file_ts < day_start,
                Op::Lte => file_ts < next_day,
                Op::Is => file_ts >= day_start && file_ts < next_day,
                Op::Contains | Op::In => false,
            }
        }
    }
}

/// Parse an ISO-8601 date (with or without time) as UTC.
fn parse_rule_date(text: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Some(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()));
    }
    None
}

fn text_eq(cond: &Condition, actual_lower: &str) -> bool {
    cond.value
        .as_text()
        .map(|s| s.to_lowercase() == actual_lower)
        .unwrap_or(false)
}

fn list_contains(cond: &Condition, actual_lower: &str) -> bool {
    cond.value
        .as_list()
        .map(|items| {
            items
                .iter()
                .any(|s| s.trim_start_matches('.').to_lowercase() == actual_lower)
        })
        .unwrap_or(false)
}

/// All conditions must match (AND). Empty condition lists never match.
pub fn rule_matches(rule: &Rule, meta: &FileMeta) -> bool {
    !rule.conditions.is_empty() && rule.conditions.iter().all(|c| condition_matches(c, meta))
}

/// Resolve a destination for a file: filename tier → custom tier → extension
/// tier. Returns the winning rule. No default fallback here — the caller
/// decides what "no rule matched" means (Super Folder → Other, watch folder →
/// leave in place).
pub fn resolve<'r>(rules: &'r [Rule], meta: &FileMeta) -> Option<&'r Rule> {
    for kind in [RuleKind::Filename, RuleKind::Custom, RuleKind::Extension] {
        if let Some(rule) = rules
            .iter()
            .filter(|r| r.enabled && r.kind == kind)
            .find(|r| rule_matches(r, meta))
        {
            return Some(rule);
        }
    }
    None
}

/// Convenience: destination of a winning rule.
pub fn resolve_destination<'r>(
    rules: &'r [Rule],
    meta: &FileMeta,
) -> Option<(&'r Rule, Destination)> {
    resolve(rules, meta).map(|r| (r, r.destination.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Category, CondValue};
    use std::path::PathBuf;

    fn meta(name: &str, ext: &str, category: Category, size: u64) -> FileMeta {
        FileMeta {
            path: PathBuf::from(format!("/tmp/{name}")),
            name_lower: name.to_lowercase(),
            ext_lower: ext.to_lowercase(),
            category,
            size_bytes: size,
            created: Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap(),
            modified: Utc.with_ymd_and_hms(2026, 9, 15, 8, 30, 0).unwrap(),
            is_symlink: false,
            is_dir: false,
            taken: None,
        }
    }

    fn cond(field: ConditionField, op: Op, value: CondValue) -> Condition {
        Condition { field, op, value }
    }

    fn rule(id: &str, kind: RuleKind, conditions: Vec<Condition>, dest: Destination) -> Rule {
        Rule {
            id: id.into(),
            name: id.into(),
            enabled: true,
            kind,
            conditions,
            destination: dest,
            rename: None,
        }
    }

    fn cat(c: Category) -> Destination {
        Destination::Category { category: c }
    }

    #[test]
    fn filename_beats_extension_tier() {
        let rules = vec![
            rule(
                "ext-pdf",
                RuleKind::Extension,
                vec![cond(
                    ConditionField::Extension,
                    Op::In,
                    CondValue::List(vec!["pdf".into()]),
                )],
                cat(Category::Documents),
            ),
            rule(
                "name-invoice",
                RuleKind::Filename,
                vec![cond(
                    ConditionField::Filename,
                    Op::Contains,
                    CondValue::Text("invoice".into()),
                )],
                cat(Category::Documents),
            ),
        ];
        let m = meta("invoice.pdf", "pdf", Category::Documents, 100);
        let (winner, _) = resolve_destination(&rules, &m).unwrap();
        assert_eq!(winner.id, "name-invoice");
    }

    #[test]
    fn first_rule_in_tier_wins_and_order_matters() {
        let rules = vec![
            rule(
                "first",
                RuleKind::Custom,
                vec![cond(
                    ConditionField::Type,
                    Op::Is,
                    CondValue::Text("video".into()),
                )],
                cat(Category::Videos),
            ),
            rule(
                "second",
                RuleKind::Custom,
                vec![cond(ConditionField::SizeMb, Op::Gt, CondValue::Num(0.0))],
                cat(Category::Archives),
            ),
        ];
        let m = meta("movie.mp4", "mp4", Category::Videos, 5_000_000);
        let (winner, _) = resolve_destination(&rules, &m).unwrap();
        assert_eq!(winner.id, "first");
        let reordered = vec![rules[1].clone(), rules[0].clone()];
        let (winner, _) = resolve_destination(&reordered, &m).unwrap();
        assert_eq!(winner.id, "second");
    }

    #[test]
    fn disabled_rules_are_skipped() {
        let mut r = rule(
            "off",
            RuleKind::Extension,
            vec![cond(
                ConditionField::Extension,
                Op::Is,
                CondValue::Text("pdf".into()),
            )],
            cat(Category::Documents),
        );
        r.enabled = false;
        let m = meta("a.pdf", "pdf", Category::Documents, 1);
        assert!(resolve(&[r], &m).is_none());
    }

    #[test]
    fn empty_conditions_never_match() {
        let r = rule("empty", RuleKind::Custom, vec![], cat(Category::Videos));
        let m = meta("a.mp4", "mp4", Category::Videos, 1);
        assert!(resolve(&[r], &m).is_none());
    }

    #[test]
    fn and_semantics_require_all_conditions() {
        let r = rule(
            "and",
            RuleKind::Custom,
            vec![
                cond(
                    ConditionField::Extension,
                    Op::Is,
                    CondValue::Text("pdf".into()),
                ),
                cond(
                    ConditionField::Filename,
                    Op::Contains,
                    CondValue::Text("invoice".into()),
                ),
            ],
            cat(Category::Documents),
        );
        let yes = meta("july-invoice.pdf", "pdf", Category::Documents, 1);
        let no = meta("report.pdf", "pdf", Category::Documents, 1);
        assert!(rule_matches(&r, &yes));
        assert!(!rule_matches(&r, &no));
    }

    #[test]
    fn size_mb_is_decimal() {
        let big = meta("big.bin", "bin", Category::Other, 2_000_000);
        let r = rule(
            "sz",
            RuleKind::Custom,
            vec![cond(ConditionField::SizeMb, Op::Gt, CondValue::Num(1.0))],
            cat(Category::Other),
        );
        assert!(rule_matches(&r, &big));
        let small = meta("small.bin", "bin", Category::Other, 500_000);
        assert!(!rule_matches(&r, &small));
    }

    #[test]
    fn date_rules_compare_days() {
        let r = rule(
            "recent",
            RuleKind::Custom,
            vec![cond(
                ConditionField::Modified,
                Op::Gt,
                CondValue::Text("2026-09-10".into()),
            )],
            cat(Category::Documents),
        );
        let m = meta("a.pdf", "pdf", Category::Documents, 1);
        assert!(rule_matches(&r, &m)); // modified 2026-09-15
        let same_day = rule(
            "same",
            RuleKind::Custom,
            vec![cond(
                ConditionField::Modified,
                Op::Is,
                CondValue::Text("2026-09-15".into()),
            )],
            cat(Category::Documents),
        );
        assert!(rule_matches(&same_day, &m));
    }

    #[test]
    fn extension_list_tolerates_leading_dots_and_case() {
        let r = rule(
            "docs",
            RuleKind::Extension,
            vec![cond(
                ConditionField::Extension,
                Op::In,
                CondValue::List(vec![".PDF".into(), "Docx".into()]),
            )],
            cat(Category::Documents),
        );
        let m = meta("X.PDF", "pdf", Category::Documents, 1);
        assert!(rule_matches(&r, &m));
    }
}
