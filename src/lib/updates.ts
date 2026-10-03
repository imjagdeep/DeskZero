// Update checks against GitHub releases (signed; see tauri.conf.json).
// Only runs when the user asks, or weekly if they turned that on.

import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

export const appVersion = () => getVersion();

export async function findUpdate(): Promise<Update | null> {
  return check({ timeout: 20_000 });
}

/** Download, verify the signature, install, then restart. */
export async function installUpdate(update: Update, onProgress: (pct: number | null) => void) {
  let total = 0;
  let done = 0;
  await update.downloadAndInstall((ev) => {
    if (ev.event === "Started") total = ev.data.contentLength ?? 0;
    else if (ev.event === "Progress") {
      done += ev.data.chunkLength;
      onProgress(total ? Math.round((done / total) * 100) : null);
    }
  });
  await relaunch();
}

const LAST_CHECK = "sf-last-update-check";
const WEEK_MS = 7 * 24 * 3600 * 1000;

/** True when a weekly check is due (and records this check). */
export function weeklyCheckDue(): boolean {
  try {
    const last = Number(localStorage.getItem(LAST_CHECK) ?? 0);
    if (Date.now() - last < WEEK_MS) return false;
    localStorage.setItem(LAST_CHECK, String(Date.now()));
    return true;
  } catch {
    // Storage unavailable: skip the automatic check, manual still works.
    return false;
  }
}
