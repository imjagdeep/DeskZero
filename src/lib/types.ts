// Mirrors of the Rust types the commands send and receive (snake_case,
// exactly as serde writes them).

export type Category =
  | "images"
  | "videos"
  | "audio"
  | "documents"
  | "spreadsheets"
  | "presentations"
  | "archives"
  | "applications"
  | "installers"
  | "disk_images"
  | "code"
  | "other";

export const CATEGORIES: { value: Category; label: string }[] = [
  { value: "images", label: "Images" },
  { value: "videos", label: "Videos" },
  { value: "audio", label: "Audio" },
  { value: "documents", label: "Documents" },
  { value: "spreadsheets", label: "Spreadsheets" },
  { value: "presentations", label: "Presentations" },
  { value: "archives", label: "Archives" },
  { value: "applications", label: "Applications" },
  { value: "installers", label: "Installers" },
  { value: "disk_images", label: "Disk Images" },
  { value: "code", label: "Code" },
  { value: "other", label: "Other" },
];

export type ConflictPolicy = "rename" | "skip" | "replace" | "ask";
export type Theme = "light" | "dark" | "system";

export interface WatchFolder {
  path: string;
  enabled: boolean;
}

export interface Settings {
  version: number;
  organize_root: string | null;
  watch_folders: WatchFolder[];
  auto_organize: boolean;
  confirm_before_move: boolean;
  watch_type_fallback: boolean;
  conflict_policy: ConflictPolicy;
  keep_history_days: number;
  theme: Theme;
  start_with_system: boolean;
  start_minimized: boolean;
  notifications: boolean;
  paused: boolean;
  ignore_patterns: string[];
  search_roots: string[];
  check_updates_weekly: boolean;
  context_menu: boolean;
}

export type RuleKind = "filename" | "custom" | "extension";
export type ConditionField = "extension" | "filename" | "type" | "size_mb" | "created" | "modified";
export type Op = "is" | "contains" | "in" | "lt" | "lte" | "gt" | "gte";
export type CondValue = string | string[] | number;

export interface Condition {
  field: ConditionField;
  op: Op;
  value: CondValue;
}

export type Destination =
  | { type: "category"; category: Category }
  | { type: "custom"; path: string };

export interface Rule {
  id: string;
  name: string;
  enabled: boolean;
  kind: RuleKind;
  conditions: Condition[];
  destination: Destination;
  rename?: string | null;
}

export interface AppConfig {
  settings: Settings;
  rules: Rule[];
  data_dir: string;
  suggested_organize_root: string | null;
}

export type PlanStatus =
  | { move: { replace: boolean } }
  | "noop"
  | { skip: { reason: string } }
  | "needs_decision";

export interface PlannedOp {
  src: string;
  dst: string;
  status: PlanStatus;
  rule_id: string | null;
  rule_name: string | null;
  size_bytes: number;
}

export interface Status {
  organized_today: number;
  needs_attention: number;
  pending: number;
  paused: boolean;
  watching: string[];
  organize_root: string | null;
  watcher_error: string | null;
}

export interface RunSummary {
  moved: number;
  failed: { path: string; error: string }[];
  skipped: number;
}

export interface Attention {
  path: string;
  reason: string;
}

export interface HistoryItem {
  src: string;
  dst: string;
}

export interface HistoryEntry {
  id: string;
  ts: string;
  kind: "move" | "undo";
  items: HistoryItem[];
  quarantined: HistoryItem[];
  failed: { src: string; error: string }[];
  undone: boolean;
}

export interface FileInfo {
  path: string;
  size_bytes: number;
  modified: string | null;
  category: Category | null;
}

export interface DupGroup {
  size_bytes: number;
  sha256: string;
  files: FileInfo[];
}

export interface RenameRow {
  src: string;
  dst: string;
  skipped: boolean;
  reason: string | null;
}

export interface FolderUsage {
  name: string;
  path: string;
  bytes: number;
  files: number;
}

export interface SearchEntry {
  path: string;
  name: string;
  kind: "app" | "folder" | "file";
}

export interface FinderInfo {
  entries: number;
  building: boolean;
  roots: string[];
  seconds_since_build: number | null;
}

export type Decision = "keep_both" | "replace" | "skip";
