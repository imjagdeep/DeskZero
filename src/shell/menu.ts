// The app's menus as data: one definition drives the in-window menu bar
// (Windows/Linux), the native menu (macOS), keyboard shortcuts, the command
// palette and the shortcuts list. Same file in DeskZero, DeskMedic and DeskFolio.

import { Menu, MenuItem, PredefinedMenuItem, Submenu, CheckMenuItem } from "@tauri-apps/api/menu";
import type { MenuIcon } from "./menuIcons";

export type NativeEdit = "undo" | "redo" | "cut" | "copy" | "paste" | "selectAll";

export interface Cmd {
  id: string;
  label: string;
  /** e.g. "Mod+O", "Mod+Shift+P", "F1", "Alt+Enter" (Mod = Ctrl, or Cmd on a Mac) */
  accel?: string;
  run: () => unknown;
  enabled?: () => boolean;
  checked?: () => boolean;
  /** On a Mac, use the system's own item (works in every text field). */
  native?: NativeEdit;
  /** Leave out of the command palette (e.g. plain clipboard items). */
  hidden?: boolean;
  /** Small icon in front of the label (in-window menus only). */
  icon?: MenuIcon;
  /** Red, for things that delete or can't be undone. */
  danger?: boolean;
}
export interface SubmenuDef {
  label: string;
  icon?: MenuIcon;
  items: Entry[];
}
export type Entry = Cmd | "-" | SubmenuDef;
export interface MenuDef {
  label: string;
  items: Entry[];
}

export const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform || navigator.userAgent);

export const isCmd = (e: Entry): e is Cmd => typeof e === "object" && "run" in e;
export const isSub = (e: Entry): e is SubmenuDef => typeof e === "object" && "items" in e && !("run" in e);

/** "Mod+Shift+S" → "Ctrl+Shift+S" (or "⇧⌘S" on a Mac). */
export function accelText(accel: string): string {
  const parts = accel.split("+");
  if (!isMac) return parts.map((p) => (p === "Mod" ? "Ctrl" : p)).join("+");
  const sym: Record<string, string> = { Mod: "⌘", Shift: "⇧", Alt: "⌥", Ctrl: "⌃" };
  const order = ["Ctrl", "Alt", "Shift", "Mod"];
  const mods = parts.slice(0, -1).sort((a, b) => order.indexOf(a) - order.indexOf(b));
  return mods.map((m) => sym[m] ?? m).join("") + parts[parts.length - 1];
}

const KEY_ALIASES: Record<string, string> = { "=": "=", Plus: "=", "+": "=", Minus: "-", Comma: ",", Slash: "/" };

/** Does this key press match the accelerator? */
export function matches(e: KeyboardEvent, accel: string): boolean {
  const parts = accel.split("+");
  const key = parts[parts.length - 1];
  const want = new Set(parts.slice(0, -1));
  const mod = isMac ? e.metaKey : e.ctrlKey;
  if (want.has("Mod") !== mod) return false;
  if (want.has("Shift") !== e.shiftKey) return false;
  if (want.has("Alt") !== e.altKey) return false;
  if (isMac && want.has("Ctrl") !== e.ctrlKey) return false;
  const k = KEY_ALIASES[key] ?? key;
  const pressed = e.key === "+" ? "=" : e.key;
  return pressed.toLowerCase() === k.toLowerCase() || e.code === `Key${k.toUpperCase()}` || e.code === `Digit${k}`;
}

/** Every command in the menus, with the menu path it lives under. */
export function allCommands(menus: MenuDef[]): { cmd: Cmd; path: string }[] {
  const out: { cmd: Cmd; path: string }[] = [];
  const walk = (items: Entry[], path: string) => {
    for (const e of items) {
      if (isCmd(e)) out.push({ cmd: e, path });
      else if (isSub(e)) walk(e.items, `${path} › ${e.label}`);
    }
  };
  for (const m of menus) walk(m.items, m.label);
  return out;
}

export async function runCmd(cmd: Cmd, onError: (e: unknown) => void) {
  if (cmd.enabled && !cmd.enabled()) return;
  try {
    await cmd.run();
  } catch (e) {
    onError(e);
  }
}

// ---------- macOS: the real menu bar at the top of the screen ----------

const NATIVE_TEXT: Record<NativeEdit, "Undo" | "Redo" | "Cut" | "Copy" | "Paste" | "SelectAll"> = {
  undo: "Undo",
  redo: "Redo",
  cut: "Cut",
  copy: "Copy",
  paste: "Paste",
  selectAll: "SelectAll",
};

type NativeItem = MenuItem | PredefinedMenuItem | Submenu | CheckMenuItem;

async function nativeItems(items: Entry[], onError: (e: unknown) => void): Promise<NativeItem[]> {
  const out: NativeItem[] = [];
  for (const e of items) {
    if (e === "-") out.push(await PredefinedMenuItem.new({ item: "Separator" }));
    else if (isSub(e)) out.push(await Submenu.new({ text: e.label, items: await nativeItems(e.items, onError) }));
    else if (isCmd(e) && e.native) out.push(await PredefinedMenuItem.new({ item: NATIVE_TEXT[e.native] }));
    else if (isCmd(e)) {
      const opts = {
        id: e.id,
        text: e.label,
        accelerator: e.accel?.replace(/^Mod\+/, "CmdOrCtrl+").replace(/\+Mod\+/, "+CmdOrCtrl+"),
        action: () => void runCmd(e, onError),
      };
      out.push(e.checked ? await CheckMenuItem.new({ ...opts, checked: e.checked() }) : await MenuItem.new(opts));
    }
  }
  return out;
}

/** Build the macOS app menu (first menu = the app's name) from the same data. */
export async function setNativeMenu(appName: string, menus: MenuDef[], onError: (e: unknown) => void) {
  const appMenu = await Submenu.new({
    text: appName,
    items: [
      await PredefinedMenuItem.new({ item: "Hide" }),
      await PredefinedMenuItem.new({ item: "HideOthers" }),
      await PredefinedMenuItem.new({ item: "ShowAll" }),
      await PredefinedMenuItem.new({ item: "Separator" }),
      await PredefinedMenuItem.new({ item: "Quit" }),
    ],
  });
  const rest: Submenu[] = [];
  for (const m of menus) rest.push(await Submenu.new({ text: m.label, items: await nativeItems(m.items, onError) }));
  const menu = await Menu.new({ items: [appMenu, ...rest] });
  await menu.setAsAppMenu();
}
