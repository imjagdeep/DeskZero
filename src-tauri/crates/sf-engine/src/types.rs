//! Shared domain types for the Super Folder engine.
//!
//! Everything here is plain data (serde-friendly) so the same types serve
//! the config files, the engine internals, the CLI harness and — later —
//! the Tauri command layer.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One of the twelve default file categories. Also used as a rule condition
/// value ("type is video") and as a destination ("category: Videos").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Images,
    Videos,
    Audio,
    Documents,
    Spreadsheets,
    Presentations,
    Archives,
    Applications,
    Installers,
    DiskImages,
    Code,
    Other,
}

impl Category {
    /// The exact subfolder name created inside the Super Folder.
    pub fn folder_name(&self) -> &'static str {
        match self {
            Category::Images => "Images",
            Category::Videos => "Videos",
            Category::Audio => "Audio",
            Category::Documents => "Documents",
            Category::Spreadsheets => "Spreadsheets",
            Category::Presentations => "Presentations",
            Category::Archives => "Archives",
            Category::Applications => "Applications",
            Category::Installers => "Installers",
            Category::DiskImages => "Disk Images",
            Category::Code => "Code",
            Category::Other => "Other",
        }
    }

    /// Lowercase singular used in rule conditions, e.g. `{ "field": "type", "op": "is", "value": "video" }`.
    pub fn rule_value(&self) -> &'static str {
        match self {
            Category::Images => "image",
            Category::Videos => "video",
            Category::Audio => "audio",
            Category::Documents => "document",
            Category::Spreadsheets => "spreadsheet",
            Category::Presentations => "presentation",
            Category::Archives => "archive",
            Category::Applications => "application",
            Category::Installers => "installer",
            Category::DiskImages => "disk_image",
            Category::Code => "code",
            Category::Other => "other",
        }
    }

    pub fn all() -> &'static [Category] {
        const ALL: &[Category] = &[
            Category::Images,
            Category::Videos,
            Category::Audio,
            Category::Documents,
            Category::Spreadsheets,
            Category::Presentations,
            Category::Archives,
            Category::Applications,
            Category::Installers,
            Category::DiskImages,
            Category::Code,
            Category::Other,
        ];
        ALL
    }

    /// Parse a rule-condition value ("video", "disk_image", …) back to a category.
    pub fn from_rule_value(s: &str) -> Option<Category> {
        Category::all()
            .iter()
            .copied()
            .find(|c| c.rule_value() == s)
    }
}

/// Which tier of the rule ordering a rule belongs to. Evaluation order:
/// filename rules → custom rules → extension rules → default category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    Filename,
    Custom,
    Extension,
}

/// Fields a rule condition can test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionField {
    Extension,
    Filename,
    Type,
    SizeMb,
    Created,
    Modified,
}

/// Comparison operators. Semantics per field are documented on `Condition::matches`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Is,
    Contains,
    In,
    Lt,
    Lte,
    Gt,
    Gte,
}

/// Condition value: plain text, a list of strings, or a number (size in MB).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CondValue {
    Text(String),
    List(Vec<String>),
    Num(f64),
}

impl CondValue {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            CondValue::Text(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_list(&self) -> Option<&[String]> {
        match self {
            CondValue::List(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_num(&self) -> Option<f64> {
        match self {
            CondValue::Num(n) => Some(*n),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub field: ConditionField,
    pub op: Op,
    pub value: CondValue,
}

/// Where a matched file goes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Destination {
    /// `<super folder>/<Category folder>/filename`
    Category { category: Category },
    /// An absolute folder path chosen by the user.
    Custom { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub kind: RuleKind,
    /// AND semantics: every condition must match. An empty list never matches
    /// (prevents accidental catch-all rules).
    pub conditions: Vec<Condition>,
    pub destination: Destination,
}

fn default_true() -> bool {
    true
}

/// What to do when the destination path already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ConflictPolicy {
    /// Default: pick a free name `file (1).pdf`, `file (2).pdf`, … Never overwrite.
    #[default]
    Rename,
    /// Leave the file where it is and flag it as needing attention.
    Skip,
    /// Overwrite the existing file. The replaced file's path is recorded in
    /// history so undo can restore it.
    Replace,
    /// Hold the operation for the user to decide (UI dialog). In watch mode
    /// this behaves like Skip plus a notification.
    Ask,
}

/// Metadata about one file, captured once at plan time.
#[derive(Debug, Clone)]
pub struct FileMeta {
    pub path: PathBuf,
    /// File name, lowercased (for case-insensitive matching).
    pub name_lower: String,
    /// Extension without the dot, lowercased (`""` when none).
    pub ext_lower: String,
    pub category: Category,
    pub size_bytes: u64,
    pub created: chrono::DateTime<chrono::Utc>,
    pub modified: chrono::DateTime<chrono::Utc>,
    /// True when the path is a symlink. Symlinks are moved as links, never followed.
    pub is_symlink: bool,
    /// True when the path is a directory.
    pub is_dir: bool,
}

/// The outcome of planning one file. Planning never touches disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    /// Move src → dst. `replace` is true only under ConflictPolicy::Replace.
    Move { replace: bool },
    /// src == dst, nothing to do.
    Noop,
    /// Do not move; `reason` explains why (no rule, conflict under Skip, guard trip).
    Skip { reason: String },
    /// Destination exists and the policy is Ask; waits for a user decision.
    NeedsDecision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedOp {
    pub src: PathBuf,
    /// Final destination. For Rename conflicts this is the probed free name,
    /// so what you see in the preview is exactly what will happen.
    pub dst: PathBuf,
    pub status: PlanStatus,
    pub rule_id: Option<String>,
    pub rule_name: Option<String>,
    pub size_bytes: u64,
}

impl PlannedOp {
    /// Ops the mover should actually perform.
    pub fn is_executable(&self) -> bool {
        matches!(
            self.status,
            PlanStatus::Move { .. } | PlanStatus::NeedsDecision
        )
    }
}

/// How a batch of files reached the planner (drives the no-rule fallback).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanMode {
    /// Files dropped into the Super Folder: unmatched files go to `Other/`.
    SuperFolder,
    /// Files found in a watch folder: unmatched files stay put ("needs attention").
    WatchFolder,
}

/// Serializable settings (settings.json).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub version: u32,
    pub super_folder: Option<PathBuf>,
    #[serde(default)]
    pub watch_folders: Vec<WatchFolder>,
    /// Super Folder: organize immediately on drop without a preview step.
    #[serde(default)]
    pub auto_organize: bool,
    /// Watch folders: show a confirmation before moving.
    #[serde(default = "default_true")]
    pub confirm_before_move: bool,
    #[serde(default)]
    pub conflict_policy: ConflictPolicy,
    /// History entries older than this many days are compacted away on startup.
    #[serde(default = "default_keep_days")]
    pub keep_history_days: u32,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default)]
    pub start_with_system: bool,
    #[serde(default)]
    pub start_minimized: bool,
    #[serde(default = "default_true")]
    pub notifications: bool,
    /// Monitoring paused by the user (tray menu).
    #[serde(default)]
    pub paused: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchFolder {
    pub path: PathBuf,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

fn default_keep_days() -> u32 {
    90
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: 1,
            super_folder: None,
            watch_folders: Vec::new(),
            auto_organize: false,
            confirm_before_move: true,
            conflict_policy: ConflictPolicy::Rename,
            keep_history_days: 90,
            theme: Theme::System,
            start_with_system: false,
            start_minimized: false,
            notifications: true,
            paused: false,
        }
    }
}

/// The root-relative anchor a Custom destination with a relative path uses.
/// (Relative custom paths are resolved against the Super Folder.)
pub const CUSTOM_DEST_BASE: &str = "super_folder";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_round_trips_through_rule_value() {
        for c in Category::all() {
            assert_eq!(Category::from_rule_value(c.rule_value()), Some(*c));
        }
        assert_eq!(Category::from_rule_value("nonsense"), None);
    }

    #[test]
    fn disk_images_folder_has_space() {
        assert_eq!(Category::DiskImages.folder_name(), "Disk Images");
    }

    #[test]
    fn settings_json_round_trip_defaults() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
        assert_eq!(s.conflict_policy, ConflictPolicy::Rename);
    }

    #[test]
    fn rule_deserializes_spec_example() {
        // Matches the rules.json example from the product spec.
        let json = r#"{
            "id": "r_1", "name": "Invoices", "enabled": true,
            "kind": "custom",
            "conditions": [
                { "field": "extension", "op": "in", "value": ["pdf"] },
                { "field": "filename", "op": "contains", "value": "invoice" }
            ],
            "destination": { "type": "category", "category": "documents" }
        }"#;
        let rule: Rule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.kind, RuleKind::Custom);
        assert!(matches!(
            &rule.destination,
            Destination::Category {
                category: Category::Documents
            }
        ));
    }
}
