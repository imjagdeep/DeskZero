// Typed wrappers over the Rust commands. Errors arrive as plain strings.

import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  Attention,
  BrowseItem,
  CleanGroup,
  Decision,
  FinderInfo,
  FolderUsage,
  SearchEntry,
  DupGroup,
  Drive,
  FileInfo,
  HistoryEntry,
  Outcome,
  PlannedOp,
  Place,
  RenameRow,
  Rule,
  RunSummary,
  Settings,
  Status,
} from "./types";

export type LabToolId = "deskzero" | "deskmedic" | "deskfolio";
export interface LabTool {
  id: LabToolId;
  installed: boolean;
}

export const api = {
  getConfig: () => invoke<AppConfig>("get_config"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  saveRules: (rules: Rule[]) => invoke<void>("save_rules", { rules }),
  getStatus: () => invoke<Status>("get_status"),
  planPaths: (paths: string[]) => invoke<PlannedOp[]>("plan_paths", { paths }),
  planWatchFolder: (path: string) => invoke<PlannedOp[]>("plan_watch_folder", { path }),
  pendingPlan: () => invoke<PlannedOp[]>("pending_plan"),
  executePlan: (plan: PlannedOp[]) => invoke<RunSummary>("execute_plan", { plan }),
  setPaused: (paused: boolean) => invoke<void>("set_paused", { paused }),
  getAttention: () => invoke<Attention[]>("get_attention"),
  clearAttention: () => invoke<void>("clear_attention"),
  getHistory: () => invoke<HistoryEntry[]>("get_history"),
  undoBatch: (id: string) => invoke<RunSummary>("undo_batch", { id }),
  findDuplicates: (folder: string) => invoke<DupGroup[]>("find_duplicates", { folder }),
  searchFiles: (folder: string, query: string) => invoke<FileInfo[]>("search_files", { folder, query }),
  /** Rename the given files with a template ({date}_{original_name}…). */
  renamePreview: (paths: string[], template: string) =>
    invoke<RenameRow[]>("rename_preview", { paths, template }),
  renameApply: (paths: string[], template: string) =>
    invoke<RunSummary>("rename_apply", { paths, template }),
  exportRules: (path: string) => invoke<number>("export_rules", { path }),
  importRules: (path: string, replace: boolean) => invoke<number>("import_rules", { path, replace }),
  resolveDecisions: (plan: PlannedOp[], decisions: [string, Decision][]) =>
    invoke<PlannedOp[]>("resolve_decisions", { plan, decisions }),
  planMoveInto: (files: string[], dest: string) => invoke<PlannedOp[]>("plan_move_into", { files, dest }),
  planOldFiles: (folder: string, days: number, dest: string) =>
    invoke<PlannedOp[]>("plan_old_files", { folder, days, dest }),
  storageOverview: () => invoke<FolderUsage[]>("storage_overview"),
  listFolder: (path: string, showHidden: boolean) => invoke<BrowseItem[]>("list_folder", { path, showHidden }),
  folderSizes: (paths: string[]) => invoke<[string, number][]>("folder_sizes", { paths }),
  places: () => invoke<Place[]>("places"),
  renameItem: (path: string, newName: string) => invoke<string>("rename_item", { path, newName }),
  newFolder: (dir: string, name: string) => invoke<string>("new_folder", { dir, name }),
  copyItems: (paths: string[], dest: string) => invoke<Outcome>("copy_items", { paths, dest }),
  /** Files and folders; recorded in History ("Moved") so it can be undone. */
  moveItems: (paths: string[], dest: string) => invoke<RunSummary>("move_items", { paths, dest }),
  recordAction: (kind: "move" | "created" | "trashed", label: string, items: { src: string; dst: string }[]) =>
    invoke<void>("record_action", { kind, label, items }),
  trashItems: (paths: string[]) => invoke<void>("trash_items", { paths }),
  thumbnails: (paths: string[]) => invoke<Record<string, string>>("thumbnails", { paths }),
  drives: () => invoke<Drive[]>("drives"),
  diskSpace: (path: string) => invoke<[number, number] | null>("disk_space", { path }),
  restoreFromTrash: (paths: string[]) => invoke<number>("restore_from_trash", { paths }),
  removeEmptyFolder: (path: string) => invoke<void>("remove_empty_folder", { path }),
  cleanupSuggestions: (dir: string, options: { sub_folders: boolean; unused_days: number | null }) =>
    invoke<CleanGroup[]>("cleanup_suggestions", { dir, options }),
  previewImage: (path: string) => invoke<string | null>("preview_image", { path }),
  previewText: (path: string) => invoke<string | null>("preview_text", { path }),
  /** macOS Quick Look; resolves false on other systems (use DeskZero's preview). */
  quickLook: (path: string) => invoke<boolean>("quick_look", { path }),
  clipboardSetFiles: (paths: string[]) => invoke<void>("clipboard_set_files", { paths }),
  clipboardGetFiles: () => invoke<string[]>("clipboard_get_files"),
  universalSearch: (query: string) => invoke<SearchEntry[]>("universal_search", { query }),
  searchIcons: (items: [string, string][]) => invoke<Record<string, string>>("search_icons", { items }),
  searchInfo: () => invoke<FinderInfo>("search_info"),
  rebuildSearch: () => invoke<void>("rebuild_search"),
  launch: (path: string) => invoke<void>("launch", { path }),
  platform: () => invoke<string>("platform"),
  takeStartupPaths: () => invoke<string[]>("take_startup_paths"),
  openLink: (id: "site" | "repo" | "issues" | "releases" | "developer" | LabToolId) => invoke<void>("open_link", { id }),
  /** The other DeskLab tools and whether each is installed here. */
  desklabTools: () => invoke<LabTool[]>("desklab_tools"),
  /** Start an installed DeskLab tool, optionally with a file for it to open. */
  openTool: (id: LabToolId, file?: string) => invoke<void>("open_tool", { id, file: file ?? null }),
  notifyUpdate: (version: string) => invoke<void>("notify_update", { version }),
  openFolder: (path: string) => invoke<void>("open_folder", { path }),
  revealFile: (path: string) => invoke<void>("reveal_file", { path }),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
