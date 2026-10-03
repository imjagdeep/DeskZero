//! Local file search: indexless walk + a tiny query language.
//!
//!   invoice                 file name contains "invoice" (case-insensitive)
//!   *.pdf  /  ext:pdf       extension is pdf
//!   type:video              classified category is video
//!   folder:scans            path contains a folder named "scans"
//!   size:>10mb  size:<1gb   size comparisons (decimal units)
//!   large                   alias for size:>100mb ("large files" just works)
//!   modified:2026-09        modified in September 2026 (prefix match)
//!   modified:>2026-09-01    modified after a date
//!   created:<2026-01-01     created before a date
//!
//! Tokens are ANDed. No index, no database, no AI — a walk plus filters.

use crate::classify;
use crate::types::Category;
use std::io;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified: Option<std::time::SystemTime>,
    pub category: Category,
}

#[derive(Debug, Clone, PartialEq)]
enum Term {
    NameContains(String),
    Extension(String),
    Type(Category),
    FolderContains(String),
    SizeGt(u64),
    SizeLt(u64),
    DateModified(DateCmp),
    DateCreated(DateCmp),
}

#[derive(Debug, Clone, PartialEq)]
enum DateCmp {
    After(chrono::NaiveDate),
    Before(chrono::NaiveDate),
    On(chrono::NaiveDate),
}

/// Parse the query. Unparseable tokens are ignored rather than erroring —
/// a search that degrades to fewer filters beats a search that errors.
fn parse_query(query: &str) -> Vec<Term> {
    let mut terms = Vec::new();
    for raw in query.split_whitespace() {
        let token = raw.trim();
        if token.is_empty() || token.eq_ignore_ascii_case("files") {
            continue; // "large files" → "large"
        }
        let lower = token.to_lowercase();
        if lower == "large" {
            terms.push(Term::SizeGt(100 * 1_000_000));
            continue;
        }
        if let Some(rest) = lower.strip_prefix("ext:") {
            let ext = rest.trim_start_matches('*').trim_start_matches('.');
            if !ext.is_empty() {
                terms.push(Term::Extension(ext.into()));
            }
            continue;
        }
        if let Some(rest) = lower.strip_prefix("*.") {
            if !rest.is_empty() {
                terms.push(Term::Extension(rest.into()));
            }
            continue;
        }
        if let Some(rest) = lower.strip_prefix("type:") {
            if let Some(cat) = Category::from_rule_value(rest) {
                terms.push(Term::Type(cat));
            }
            continue;
        }
        if let Some(rest) = lower.strip_prefix("folder:") {
            terms.push(Term::FolderContains(rest.into()));
            continue;
        }
        if let Some(rest) = lower.strip_prefix("size:") {
            if let Some((cmp, bytes)) = parse_size(rest) {
                match cmp {
                    Cmp::Gt => terms.push(Term::SizeGt(bytes)),
                    Cmp::Lt => terms.push(Term::SizeLt(bytes)),
                }
            }
            continue;
        }
        for (prefix, which) in [("modified:", 0u8), ("created:", 1u8)] {
            if let Some(rest) = lower.strip_prefix(prefix) {
                if let Some(cmp) = parse_date_cmp(rest) {
                    terms.push(if which == 0 {
                        Term::DateModified(cmp)
                    } else {
                        Term::DateCreated(cmp)
                    });
                }
                break;
            }
        }
        // A token starting with a key: prefix that failed to parse is
        // dropped (not treated as a name search for "size:>xyz").
        if lower.contains(':') {
            continue;
        }
        terms.push(Term::NameContains(lower));
    }
    terms
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Cmp {
    Gt,
    Lt,
}

fn parse_size(s: &str) -> Option<(Cmp, u64)> {
    if let Some(rest) = s.strip_prefix('>') {
        return Some((Cmp::Gt, rest.unit_bytes()?));
    }
    if let Some(rest) = s.strip_prefix('<') {
        return Some((Cmp::Lt, rest.unit_bytes()?));
    }
    // Bare "10mb" means "at least 10 MB" (consistent with rule size_mb).
    Some((Cmp::Gt, s.unit_bytes()?))
}

/// Search a folder tree. Returns files only (no directories), newest
/// modification first.
pub fn search(root: &Path, query: &str) -> io::Result<Vec<SearchHit>> {
    let terms = parse_query(query);
    let mut hits = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let md = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let category = classify::classify(&name);
        let modified = md.modified().ok();
        let created = md.created().ok();

        let ok = terms.iter().all(|t| match t {
            Term::NameContains(s) => name.contains(s),
            Term::Extension(e) => classify::extension_of(&name) == *e,
            Term::Type(c) => category == *c,
            Term::FolderContains(f) => path
                .parent()
                .map(|p| {
                    p.components()
                        .any(|c| c.as_os_str().to_string_lossy().to_lowercase().contains(f))
                })
                .unwrap_or(false),
            Term::SizeGt(n) => md.len() > *n,
            Term::SizeLt(n) => md.len() < *n,
            Term::DateModified(cmp) => date_matches(cmp, modified),
            Term::DateCreated(cmp) => date_matches(cmp, created),
        });
        if ok {
            hits.push(SearchHit {
                path: path.to_path_buf(),
                size_bytes: md.len(),
                modified,
                category,
            });
        }
    }
    hits.sort_by_key(|h| std::cmp::Reverse(h.modified));
    Ok(hits)
}

fn date_matches(cmp: &DateCmp, ts: Option<std::time::SystemTime>) -> bool {
    let Some(ts) = ts else { return false };
    let dt: chrono::DateTime<chrono::Utc> = ts.into();
    let date = dt.date_naive();
    match cmp {
        DateCmp::After(d) => date > *d,
        DateCmp::Before(d) => date < *d,
        DateCmp::On(d) => date == *d,
    }
}

fn parse_date_cmp(s: &str) -> Option<DateCmp> {
    if let Some(rest) = s.strip_prefix('>') {
        return Some(DateCmp::After(
            chrono::NaiveDate::parse_from_str(rest, "%Y-%m-%d").ok()?,
        ));
    }
    if let Some(rest) = s.strip_prefix('<') {
        return Some(DateCmp::Before(
            chrono::NaiveDate::parse_from_str(rest, "%Y-%m-%d").ok()?,
        ));
    }
    // "2026-09" prefix matching: pad to a full date.
    let rest = if s.len() == 7 {
        format!("{s}-01")
    } else {
        s.to_string()
    };
    Some(DateCmp::On(
        chrono::NaiveDate::parse_from_str(&rest, "%Y-%m-%d").ok()?,
    ))
}

/// Helpers for size parsing, namespaced to avoid polluting the module.
trait SizeText {
    fn unit_bytes(&self) -> Option<u64>;
}

impl SizeText for str {
    fn unit_bytes(&self) -> Option<u64> {
        let digits: String = self
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let unit: String = self[digits.len()..]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let n: f64 = digits.parse().ok()?;
        let mult = match unit.to_lowercase().as_str() {
            "" | "b" => 1.0,
            "k" | "kb" => 1_000.0,
            "m" | "mb" => 1_000_000.0,
            "g" | "gb" => 1_000_000_000.0,
            "t" | "tb" => 1_000_000_000_000.0,
            _ => return None,
        };
        Some((n * mult) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("scans")).unwrap();
        fs::write(root.join("invoice-july.pdf"), b"aaaa").unwrap();
        fs::write(root.join("scans").join("invoice-aug.pdf"), b"bb").unwrap();
        fs::write(root.join("movie.mp4"), vec![0u8; 2_500]).unwrap();
        fs::write(root.join("song.mp3"), b"ccc").unwrap();
        tmp
    }

    #[test]
    fn name_search_is_case_insensitive_substring() {
        let tmp = tree();
        let hits = search(tmp.path(), "INVOICE").unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn ext_and_glob_forms_agree() {
        let tmp = tree();
        let a = search(tmp.path(), "ext:pdf").unwrap();
        let b = search(tmp.path(), "*.pdf").unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(a.len(), b.len());
    }

    #[test]
    fn folder_term_narrows_by_parent() {
        let tmp = tree();
        let hits = search(tmp.path(), "invoice folder:scans").unwrap();
        assert_eq!(hits.len(), 1);
        assert!(
            hits[0].path.ends_with("scans/invoice-aug.pdf")
                || hits[0].path.ends_with("scans\\invoice-aug.pdf")
        );
    }

    #[test]
    fn size_comparisons_use_decimal_units() {
        let tmp = tree();
        let big = search(tmp.path(), "size:>1kb").unwrap();
        assert_eq!(big.len(), 1); // the 2,500-byte mp4
        let small = search(tmp.path(), "size:<1kb").unwrap();
        assert_eq!(small.len(), 3);
    }

    #[test]
    fn type_term_uses_classification() {
        let tmp = tree();
        let hits = search(tmp.path(), "type:audio").unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("song.mp3"));
    }

    #[test]
    fn tokens_are_anded() {
        let tmp = tree();
        let hits = search(tmp.path(), "invoice ext:pdf folder:scans").unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn garbage_tokens_degrade_gracefully() {
        let tmp = tree();
        let hits = search(tmp.path(), "invoice size:>bogus ext:").unwrap();
        assert_eq!(hits.len(), 2); // size term dropped, ext term dropped
    }

    #[test]
    fn size_unit_parser() {
        assert_eq!("10mb".unit_bytes(), Some(10_000_000));
        assert_eq!("1.5gb".unit_bytes(), Some(1_500_000_000));
        assert_eq!("500kb".unit_bytes(), Some(500_000));
        assert_eq!("42".unit_bytes(), Some(42));
        assert_eq!("10xb".unit_bytes(), None);
    }
}
