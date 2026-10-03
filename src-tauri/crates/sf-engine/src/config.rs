//! Configuration persistence: settings.json and rules.json.
//!
//! All writes are atomic (temp file + rename) so a crash mid-save — likely on
//! a 24/7 PC — never corrupts state. Everything lives in one directory so the
//! engine is fully testable against a tempdir.

use crate::types::{Rule, Settings};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const SETTINGS_FILE: &str = "settings.json";
pub const RULES_FILE: &str = "rules.json";
pub const HISTORY_FILE: &str = "history.jsonl";

/// Everything the engine needs, loaded from disk.
#[derive(Debug, Clone)]
pub struct Config {
    pub settings: Settings,
    pub rules: Vec<Rule>,
    /// Where settings/rules/history live. Injected by the caller (app config
    /// dir in production, tempdir in tests).
    pub data_dir: PathBuf,
}

impl Config {
    /// Load from `data_dir`, filling defaults for missing or unreadable files.
    /// A corrupt settings file is never fatal: defaults keep the app running
    /// and the bad file is left in place for the user to inspect.
    pub fn load(data_dir: impl AsRef<Path>) -> Config {
        let data_dir = data_dir.as_ref().to_path_buf();
        let settings = read_json(&data_dir.join(SETTINGS_FILE)).unwrap_or_default();
        let rules: Vec<Rule> = read_json::<RulesFile>(&data_dir.join(RULES_FILE))
            .map(|f| f.rules)
            .unwrap_or_default();
        Config {
            settings,
            rules,
            data_dir,
        }
    }

    /// The per-user config dir used in production:
    /// `%APPDATA%\deskzero` on Windows, `~/.config/deskzero` on Linux.
    pub fn default_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("deskzero"))
    }

    pub fn save_settings(&self) -> io::Result<()> {
        write_atomic(&self.data_dir.join(SETTINGS_FILE), &self.settings)
    }

    pub fn save_rules(&self) -> io::Result<()> {
        write_atomic(
            &self.data_dir.join(RULES_FILE),
            &RulesFile {
                rules: self.rules.clone(),
            },
        )
    }

    pub fn history_path(&self) -> PathBuf {
        self.data_dir.join(HISTORY_FILE)
    }
}

/// rules.json envelope: `{ "version": 1, "rules": [...] }`.
#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
struct RulesFile {
    rules: Vec<Rule>,
}

fn read_json<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Option<T> {
    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("ignoring corrupt {}: {e}", path.display());
                None
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Some(T::default()),
        Err(e) => {
            tracing::warn!("cannot read {}: {e}", path.display());
            None
        }
    }
}

/// Serialize `value` to `path` atomically: write `<path>.tmp` then rename.
/// On Windows rename fails if the destination exists, so remove first —
/// still atomic enough: the old file survives until the new one is complete.
pub fn write_atomic(path: &Path, value: &impl serde::Serialize) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp, text)?;
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
    use crate::types::{CondValue, Condition, ConditionField, Destination, Op, RuleKind};

    #[test]
    fn load_missing_files_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::load(dir.path());
        assert_eq!(cfg.settings, Settings::default());
        assert!(cfg.rules.is_empty());
    }

    #[test]
    fn corrupt_settings_fall_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(SETTINGS_FILE), "{ not json !!!").unwrap();
        let cfg = Config::load(dir.path());
        assert_eq!(cfg.settings, Settings::default());
    }

    #[test]
    fn settings_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::load(dir.path());
        cfg.settings.organize_root = Some(PathBuf::from("D:\\DeskZero"));
        cfg.save_settings().unwrap();

        let reloaded = Config::load(dir.path());
        assert_eq!(
            reloaded.settings.organize_root,
            Some(PathBuf::from("D:\\DeskZero"))
        );
    }

    #[test]
    fn rules_round_trip_matches_spec_format() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::load(dir.path());
        cfg.rules = vec![Rule {
            id: "r_1".into(),
            name: "Invoices".into(),
            enabled: true,
            kind: RuleKind::Custom,
            conditions: vec![
                Condition {
                    field: ConditionField::Extension,
                    op: Op::In,
                    value: CondValue::List(vec!["pdf".into()]),
                },
                Condition {
                    field: ConditionField::Filename,
                    op: Op::Contains,
                    value: CondValue::Text("invoice".into()),
                },
            ],
            destination: Destination::Category {
                category: crate::types::Category::Documents,
            },
            rename: None,
        }];
        cfg.save_rules().unwrap();

        let text = fs::read_to_string(dir.path().join(RULES_FILE)).unwrap();
        assert!(text.contains("\"rules\""));
        let reloaded = Config::load(dir.path());
        assert_eq!(reloaded.rules, cfg.rules);
    }

    #[test]
    fn atomic_write_leaves_no_tmp_behind() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::load(dir.path());
        cfg.save_settings().unwrap();
        let mut entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        entries.sort();
        assert_eq!(entries, vec![SETTINGS_FILE.to_string()]);
    }
}
