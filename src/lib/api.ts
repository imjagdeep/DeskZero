// Typed wrappers over the Rust commands. Errors arrive as plain strings.

import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  Attention,
  DupGroup,
  FileInfo,
  HistoryEntry,
  PlannedOp,
  RenameRow,
  Rule,
  RunSummary,
  Settings,
  Status,
} from "./types";

export const api = {
  getConfig: () => invoke<AppConfig>("get_config"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  saveRules: (rules: Rule[]) => invoke<void>("save_rules", { rules }),
  getStatus: () => invoke<Status>("get_status"),
  planPaths: (paths: string[]) => invoke<PlannedOp[]>("plan_paths", { paths }),
  pendingPlan: () => invoke<PlannedOp[]>("pending_plan"),
  executePlan: (plan: PlannedOp[]) => invoke<RunSummary>("execute_plan", { plan }),
  setPaused: (paused: boolean) => invoke<void>("set_paused", { paused }),
  getAttention: () => invoke<Attention[]>("get_attention"),
  clearAttention: () => invoke<void>("clear_attention"),
  getHistory: () => invoke<HistoryEntry[]>("get_history"),
  undoBatch: (id: string) => invoke<RunSummary>("undo_batch", { id }),
  findDuplicates: (folder: string) => invoke<DupGroup[]>("find_duplicates", { folder }),
  searchFiles: (folder: string, query: string) => invoke<FileInfo[]>("search_files", { folder, query }),
  renamePreview: (folder: string, template: string) =>
    invoke<RenameRow[]>("rename_preview", { folder, template }),
  renameApply: (folder: string, template: string) =>
    invoke<RunSummary>("rename_apply", { folder, template }),
  openFolder: (path: string) => invoke<void>("open_folder", { path }),
  revealFile: (path: string) => invoke<void>("reveal_file", { path }),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
