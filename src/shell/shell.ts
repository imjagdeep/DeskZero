// Window-level state shared by the menu bar, sidebar and dialogs: compact
// sidebar, zoom, full screen, which shell dialog is open.
// Same file in DeskZero, DeskMedic and DeskFolio.

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSyncExternalStore } from "react";

function store<T>(initial: T) {
  let value = initial;
  const subs = new Set<() => void>();
  return {
    get: () => value,
    set: (v: T) => {
      value = v;
      subs.forEach((f) => f());
    },
    subscribe: (f: () => void) => {
      subs.add(f);
      return () => subs.delete(f);
    },
  };
}

function readPref(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    // Storage blocked: the default is fine.
    return null;
  }
}
function writePref(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not remembered next time; nothing else depends on it.
  }
}

// ---------- compact sidebar (icons only) ----------
const COMPACT_KEY = "desklab-sidebar-compact";
const compactStore = store(readPref(COMPACT_KEY) === "1");
export const useCompactSidebar = () => useSyncExternalStore(compactStore.subscribe, compactStore.get);
export const isCompactSidebar = () => compactStore.get();
export function setCompactSidebar(on: boolean) {
  compactStore.set(on);
  writePref(COMPACT_KEY, on ? "1" : "0");
}
export const toggleSidebar = () => setCompactSidebar(!compactStore.get());

// ---------- zoom (the whole window, like a browser) ----------
const ZOOM_KEY = "desklab-zoom";
const ZOOMS = [0.8, 0.9, 1, 1.1, 1.25, 1.4, 1.6];
const zoomStore = store(Number(readPref(ZOOM_KEY)) || 1);
export const useZoom = () => useSyncExternalStore(zoomStore.subscribe, zoomStore.get);
export async function setZoom(z: number) {
  await getCurrentWebview().setZoom(z);
  zoomStore.set(z);
  writePref(ZOOM_KEY, String(z));
}
export const applySavedZoom = () => (zoomStore.get() === 1 ? Promise.resolve() : setZoom(zoomStore.get()));
export function zoomStep(dir: 1 | -1) {
  const i = ZOOMS.findIndex((z) => z >= zoomStore.get() - 0.001);
  const next = ZOOMS[Math.min(ZOOMS.length - 1, Math.max(0, (i < 0 ? 2 : i) + dir))];
  return setZoom(next);
}

// ---------- full screen ----------
export async function toggleFullscreen() {
  const w = getCurrentWindow();
  await w.setFullscreen(!(await w.isFullscreen()));
}

// ---------- which shell dialog is showing ----------
export type ShellDialog = null | "about" | "shortcuts" | "palette";
const dialogStore = store<ShellDialog>(null);
export const useShellDialog = () => useSyncExternalStore(dialogStore.subscribe, dialogStore.get);
export const openShellDialog = (d: ShellDialog) => dialogStore.set(d);
export const closeShellDialog = () => dialogStore.set(null);

// ---------- edit commands on whatever text field has focus ----------
const TEXT_INPUTS = new Set(["text", "search", "password", "email", "number", "url", "tel"]);
const editable = () => {
  const el = document.activeElement;
  if (el instanceof HTMLInputElement) return TEXT_INPUTS.has(el.type) ? el : null;
  return el instanceof HTMLTextAreaElement || (el instanceof HTMLElement && el.isContentEditable) ? el : null;
};
export const textFieldFocused = () => editable() !== null;

export function editCommand(what: "cut" | "copy" | "selectAll" | "undo" | "redo") {
  const el = editable();
  if (what === "selectAll" && (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) {
    el.select();
    return;
  }
  // Still the only way to drive the field's own clipboard and undo from a menu.
  document.execCommand(what);
}

export async function pasteText() {
  const el = editable();
  if (!el) return;
  // Read through the app, not the browser: no "allow clipboard?" prompt.
  const text = await readText();
  el.focus();
  document.execCommand("insertText", false, text);
}
