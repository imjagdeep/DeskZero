// Undo for the file browser (Ctrl/Cmd+Z and Edit › Undo). Every action is
// recorded in History next to the organizer's batches, so it survives a
// restart and can also be undone from the History page.

import { listen } from "@tauri-apps/api/event";
import { useSyncExternalStore } from "react";
import { api } from "./api";
import { createStore, toast, toastError } from "./store";
import type { HistoryEntry } from "./types";

/** What the browser records: a rename (move), new folders or copies
 *  (created), or items moved to the Trash (trashed). Moves of files and
 *  folders are recorded by the move itself. */
export type Recorded =
  | { kind: "rename"; from: string; to: string }
  | { kind: "created"; label: "New folder" | "Copied"; paths: string[] }
  | { kind: "trashed"; paths: string[] };

const latest = createStore<HistoryEntry | null>(null);
/** Bumped whenever History changes, so open folders reload. */
const version = createStore(0);

export const useUndoVersion = () => useSyncExternalStore(version.subscribe, version.get);
export const canUndo = () => latest.get() !== null;

/** The newest entry that can still be undone. */
const undoable = (all: HistoryEntry[]) => all.find((e) => e.kind !== "undo" && !e.undone && e.items.length > 0) ?? null;

async function reload() {
  try {
    latest.set(undoable(await api.getHistory()));
  } catch {
    latest.set(null); // the Undo command just shows as unavailable
  }
  version.set(version.get() + 1);
}

let started = false;
/** Follow History; call once at start. */
export function startUndo() {
  if (started) return;
  started = true;
  void reload();
  void listen("history-changed", () => void reload());
}

export async function record(r: Recorded) {
  try {
    if (r.kind === "rename") await api.recordAction("move", "Renamed", [{ src: r.from, dst: r.to }]);
    else if (r.kind === "created") await api.recordAction("created", r.label, r.paths.map((p) => ({ src: p, dst: p })));
    else await api.recordAction("trashed", "Moved to Trash", r.paths.map((p) => ({ src: p, dst: p })));
  } catch (e) {
    toastError(e);
  }
}

export function describe(e: HistoryEntry): string {
  const n = e.items.length;
  const what = `${n} item${n === 1 ? "" : "s"}`;
  if (e.label === "Renamed" && n === 1) return "Renamed";
  if (e.label) return `${e.label} ${what}`;
  return `Organized ${what}`;
}

/** Undo the most recent action, from the browser or the organizer. */
export async function undoLast() {
  const e = undoable(await api.getHistory().catch(() => []));
  if (!e) return toast("Nothing to undo");
  try {
    const r = await api.undoBatch(e.id);
    toast(`Undid: ${describe(e)}${r.failed.length ? ` (${r.failed.length} couldn't be)` : ""}`, r.failed.length > 0);
  } catch (err) {
    toastError(err);
  }
}
