// Undo for what the file browser does (Ctrl/Cmd+Z and Edit › Undo). One
// list for the session; each entry knows how to reverse itself. Organizer
// batches keep their own undo in History.

import { useSyncExternalStore } from "react";
import { api } from "./api";
import { fileName, parentDir } from "./format";
import { createStore, toast, toastError } from "./store";

export type UndoEntry =
  | { kind: "rename"; from: string; to: string }
  | { kind: "new-folder"; path: string }
  | { kind: "copy"; created: string[] }
  /** Files moved through the mover, and folders renamed into place. */
  | { kind: "move"; pairs: { from: string; to: string; dir: boolean }[] }
  | { kind: "trash"; paths: string[] };

const stack = createStore<UndoEntry[]>([]);
/** Bumped after every undo so open folders reload. */
const version = createStore(0);

export const useUndoVersion = () => useSyncExternalStore(version.subscribe, version.get);
export const canUndo = () => stack.get().length > 0;

export function pushUndo(e: UndoEntry) {
  stack.set([...stack.get().slice(-49), e]);
}

const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

function describe(e: UndoEntry): string {
  switch (e.kind) {
    case "rename":
      return `rename of ${fileName(e.to)}`;
    case "new-folder":
      return `new folder ${fileName(e.path)}`;
    case "copy":
      return `copy of ${plural(e.created.length, "item")}`;
    case "move":
      return `move of ${plural(e.pairs.length, "item")}`;
    case "trash":
      return `delete of ${plural(e.paths.length, "item")}`;
  }
}

async function reverse(e: UndoEntry) {
  switch (e.kind) {
    case "rename":
      await api.renameItem(e.to, fileName(e.from));
      return;
    case "new-folder":
      await api.removeEmptyFolder(e.path);
      return;
    case "copy":
      // The copies go to the Trash, so even this can be taken back.
      await api.trashItems(e.created);
      return;
    case "move": {
      const files = e.pairs.filter((p) => !p.dir);
      for (const p of files) {
        await api.executePlan(await api.planMoveInto([p.to], parentDir(p.from)));
      }
      for (const p of e.pairs.filter((x) => x.dir)) {
        await api.moveFolders([p.to], parentDir(p.from));
      }
      return;
    }
    case "trash":
      await api.restoreFromTrash(e.paths);
      return;
  }
}

/** Undo the most recent browser action. */
export async function undoLast() {
  const all = stack.get();
  const last = all[all.length - 1];
  if (!last) return toast("Nothing to undo");
  stack.set(all.slice(0, -1));
  try {
    await reverse(last);
    toast(`Undid the ${describe(last)}`);
  } catch (e) {
    toastError(e);
  }
  version.set(version.get() + 1);
}
