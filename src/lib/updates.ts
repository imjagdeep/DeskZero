// In-app updates from GitHub releases, signed (public key in tauri.conf.json).
// Automatic mode: check soon after start and then daily, download in the
// background, then offer a one-click restart. Same code in DeskMedic.

import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { useSyncExternalStore } from "react";
import { errorText } from "./api";
import { createStore } from "./store";

export type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "latest" }
  | { kind: "downloading"; version: string; pct: number | null }
  | { kind: "ready"; version: string; notes: string }
  | { kind: "installing"; version: string }
  | { kind: "error"; message: string };

const store = createStore<UpdateState>({ kind: "idle" });
export const useUpdate = () => useSyncExternalStore(store.subscribe, store.get);

let pending: Update | null = null;

/**
 * Look for a newer version and, if there is one, download and verify it.
 * Returns the version that is ready to install, or null. A manual check shows
 * errors; an automatic one stays quiet (offline is normal).
 */
export async function checkAndDownload(manual: boolean): Promise<string | null> {
  const cur = store.get().kind;
  if (cur === "checking" || cur === "downloading" || cur === "installing") return null;
  if (cur === "ready" && pending) return pending.version;
  store.set({ kind: "checking" });
  try {
    const u = await check({ timeout: 20_000 });
    if (!u) {
      store.set({ kind: manual ? "latest" : "idle" });
      return null;
    }
    store.set({ kind: "downloading", version: u.version, pct: 0 });
    let total = 0;
    let done = 0;
    await u.download((ev) => {
      if (ev.event === "Started") total = ev.data.contentLength ?? 0;
      else if (ev.event === "Progress") {
        done += ev.data.chunkLength;
        store.set({ kind: "downloading", version: u.version, pct: total ? Math.round((done / total) * 100) : null });
      }
    });
    pending = u;
    store.set({ kind: "ready", version: u.version, notes: u.body ?? "" });
    return u.version;
  } catch (e) {
    store.set(manual ? { kind: "error", message: errorText(e) } : { kind: "idle" });
    return null;
  }
}

/** Install the downloaded update and restart (the one click). */
export async function installNow() {
  if (!pending) return;
  const version = pending.version;
  store.set({ kind: "installing", version });
  try {
    await pending.install();
    await relaunch();
  } catch (e) {
    store.set({ kind: "error", message: errorText(e) });
  }
}

const LAST_AUTO = "last-auto-update-check";
const DAY_MS = 24 * 3600 * 1000;

/** True when the daily automatic check is due (and records it). */
export function dailyCheckDue(): boolean {
  try {
    const last = Number(localStorage.getItem(LAST_AUTO) ?? 0);
    if (Date.now() - last < DAY_MS) return false;
    localStorage.setItem(LAST_AUTO, String(Date.now()));
    return true;
  } catch {
    // Storage unavailable: check anyway, it is only one request.
    return true;
  }
}
