// The file browser DeskZero opens on: a folder (Downloads by default) with
// its sub-folders, and its loose files grouped by kind or date. Grouping is
// only a view: nothing moves until "Sort into folders", which goes through
// the usual preview and can be undone.

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useId, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { CleanupSheet } from "../components/CleanupSheet";
import { ContextMenu, type MenuItem } from "../components/ContextMenu";
import { PreviewSheet } from "../components/PreviewSheet";
import { RenameSheet } from "../components/RenameSheet";
import {
  AppGridIcon,
  ArchiveIcon,
  BackIcon,
  BroomIcon,
  CodeIcon,
  DiscIcon,
  DocumentIcon,
  FilmIcon,
  ForwardIcon,
  ImageIcon,
  ListIcon,
  MoreIcon,
  MusicIcon,
  NewFolderIcon,
  PackageIcon,
  PdfIcon,
  RefreshIcon,
  SearchIcon,
  SlidesIcon,
  SparkleIcon,
  TableIcon,
  UpIcon,
} from "../components/Icons";
import { api, errorText } from "../lib/api";
import { dateTime, fileName, humanSize } from "../lib/format";
import { createStore, openPreview, refreshConfig, toast, toastError, usePreview } from "../lib/store";
import type { BrowseItem, Category, Outcome, PlannedOp, Settings } from "../lib/types";
import { canUndo, record, undoLast, useUndoVersion } from "../lib/undo";
import { textFieldFocused } from "../shell/shell";

/** Where the browser is: a folder, optionally narrowed to one kind of file. */
export interface Location {
  path: string;
  kind: KindKey | null;
}

type KindKey = Category | "pdfs";
type Grouping = "none" | "kind" | "date";
type View = "grid" | "list";
type IconC = (p: { size?: number }) => React.ReactElement;

/** Kinds in the order the tiles show, each with its colour. PDFs get their
 *  own tile (most Downloads have many); the rest follow DeskZero's categories. */
const KINDS: { key: KindKey; label: string; Icon: IconC; color: string }[] = [
  { key: "videos", label: "Videos", Icon: FilmIcon, color: "#2f8a9e" },
  { key: "images", label: "Images", Icon: ImageIcon, color: "#e2892b" },
  { key: "pdfs", label: "PDFs", Icon: PdfIcon, color: "#d2443f" },
  { key: "audio", label: "Audio", Icon: MusicIcon, color: "#c8336f" },
  { key: "documents", label: "Documents", Icon: DocumentIcon, color: "#3b73d9" },
  { key: "spreadsheets", label: "Spreadsheets", Icon: TableIcon, color: "#2e9b5c" },
  { key: "presentations", label: "Presentations", Icon: SlidesIcon, color: "#d9662b" },
  { key: "archives", label: "Archives", Icon: ArchiveIcon, color: "#8a6a4a" },
  { key: "installers", label: "Installers", Icon: PackageIcon, color: "#5d6878" },
  { key: "disk_images", label: "Disk Images", Icon: DiscIcon, color: "#6b6f8a" },
  { key: "applications", label: "Apps & Scripts", Icon: AppGridIcon, color: "#7a55c7" },
  { key: "code", label: "Code", Icon: CodeIcon, color: "#4a7f8c" },
  { key: "other", label: "Other", Icon: MoreIcon, color: "#8b8e95" },
];
const kindOf = (key: KindKey) => KINDS.find((k) => k.key === key) ?? KINDS[KINDS.length - 1];

function kindKey(item: BrowseItem): KindKey {
  if (/\.pdf$/i.test(item.name)) return "pdfs";
  return item.category ?? "other";
}

const DATE_GROUPS = ["Today", "Yesterday", "Previous 7 days", "Previous 30 days", "Earlier this year", "Older", "Unknown date"];

function dateGroup(iso: string | null, now = new Date()): string {
  if (!iso) return "Unknown date";
  const d = new Date(iso);
  if (isNaN(d.getTime())) return "Unknown date";
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const day = 86_400_000;
  const t = d.getTime();
  if (t >= startOfToday) return "Today";
  if (t >= startOfToday - day) return "Yesterday";
  if (t >= startOfToday - 7 * day) return "Previous 7 days";
  if (t >= startOfToday - 30 * day) return "Previous 30 days";
  if (d.getFullYear() === now.getFullYear()) return "Earlier this year";
  return "Older";
}

/** One tile or row on screen. */
type Entry =
  | { type: "folder"; item: BrowseItem }
  | { type: "kind"; key: KindKey; files: BrowseItem[] }
  | { type: "file"; item: BrowseItem };

const entryId = (e: Entry) => (e.type === "kind" ? `kind:${e.key}` : e.item.path);

function pref<T extends string>(key: string, fallback: T, allowed: readonly T[]): T {
  try {
    const v = localStorage.getItem(key);
    return v && (allowed as readonly string[]).includes(v) ? (v as T) : fallback;
  } catch {
    return fallback;
  }
}
function savePref(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not saved: the choice still applies until DeskZero restarts.
  }
}

/** Split a path into clickable breadcrumb parts. */
function crumbs(path: string): { label: string; path: string }[] {
  const win = /^[A-Za-z]:[\\/]/.test(path) || path.startsWith("\\\\");
  const sep = win ? "\\" : "/";
  const parts = path.split(/[\\/]+/).filter(Boolean);
  const out: { label: string; path: string }[] = [];
  let acc = win ? "" : "/";
  if (!win) out.push({ label: "/", path: "/" });
  parts.forEach((p, i) => {
    acc = win ? (i === 0 ? `${p}${sep}` : `${acc}${acc.endsWith(sep) ? "" : sep}${p}`) : `${acc}${acc.endsWith("/") ? "" : "/"}${p}`;
    out.push({ label: p, path: acc });
  });
  return out;
}

function parentOf(path: string): string | null {
  const c = crumbs(path);
  return c.length > 1 ? c[c.length - 2].path : null;
}

const joinPath = (dir: string, name: string) => {
  const sep = dir.includes("\\") ? "\\" : "/";
  return dir.endsWith(sep) ? dir + name : dir + sep + name;
};

/** Files cut or copied in the browser, kept while you move between folders.
 *  (DeskZero's own clipboard: pasting into Explorer or Finder isn't wired up.) */
const clipStore = createStore<{ mode: "copy" | "cut"; items: { path: string; dir: boolean }[] } | null>(null);
const useClip = () => useSyncExternalStore(clipStore.subscribe, clipStore.get);

/** Edit › Find files: put the cursor in the search box (the browser may
 *  still be appearing, so try for a few frames). */
export function focusBrowserSearch(tries = 10) {
  const box = document.querySelector<HTMLInputElement>(".bz-filter input");
  if (box) {
    box.focus();
    box.select();
  } else if (tries > 0) requestAnimationFrame(() => focusBrowserSearch(tries - 1));
}

const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

export function Browse({
  loc,
  go,
  back,
  forward,
  canBack,
  canForward,
  platform,
  settings,
}: {
  loc: Location;
  go: (l: Location) => void;
  back: () => void;
  forward: () => void;
  canBack: boolean;
  canForward: boolean;
  platform: string;
  settings: Settings;
}) {
  const [items, setItems] = useState<BrowseItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sizes, setSizes] = useState<Record<string, number>>({});
  const [icons, setIcons] = useState<Record<string, string>>({});
  const [thumbs, setThumbs] = useState<Record<string, string>>({});
  const [space, setSpace] = useState<[number, number] | null>(null);
  const [grouping, setGrouping] = useState<Grouping>(() => pref("dz.grouping", "kind", ["none", "kind", "date"] as const));
  const [view, setView] = useState<View>(() => pref("dz.view", "grid", ["grid", "list"] as const));
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const anchor = useRef<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [confirmTrash, setConfirmTrash] = useState<string[] | null>(null);
  const [reload, setReload] = useState(0);
  const [peeking, setPeeking] = useState<string | null>(null);
  const [cleaning, setCleaning] = useState(false);
  const [dropping, setDropping] = useState(false);
  const [batchRename, setBatchRename] = useState<string[] | null>(null);
  const undoVersion = useUndoVersion();
  const clip = useClip();
  const preview = usePreview();
  const hadPreview = useRef(false);
  const root = useRef<HTMLDivElement>(null);
  const mac = platform === "macos";
  const binName = platform === "windows" ? "Recycle Bin" : "Trash";
  const refresh = () => setReload((n) => n + 1);

  // After "Sort into folders" (or any organize) closes, show the result.
  useEffect(() => {
    if (preview) hadPreview.current = true;
    else if (hadPreview.current) {
      hadPreview.current = false;
      refresh();
    }
  }, [preview]);

  useEffect(() => {
    let live = true;
    setError(null);
    api
      .listFolder(loc.path, settings.show_hidden)
      .then((list) => {
        if (!live) return;
        setItems(list);
        const dirs = list.filter((i) => i.is_dir).map((i) => i.path);
        if (dirs.length && settings.folder_sizes) {
          api
            .folderSizes(dirs)
            .then((pairs) => live && setSizes(Object.fromEntries(pairs)))
            .catch(() => undefined); // sizes are a nicety; tiles still show counts
        }
      })
      .catch((e) => {
        if (!live) return;
        setItems([]);
        setError(errorText(e));
      });
    api
      .diskSpace(loc.path)
      .then((s) => live && setSpace(s))
      .catch(() => live && setSpace(null));
    return () => {
      live = false;
    };
  }, [loc.path, reload, undoVersion, settings.show_hidden, settings.folder_sizes]);

  useEffect(() => {
    setFilter("");
    setSelected(new Set());
    setRenaming(null);
    anchor.current = null;
  }, [loc.path, loc.kind]);

  const needle = filter.trim().toLowerCase();
  const visible = useMemo(
    () => (items ?? []).filter((i) => !needle || i.name.toLowerCase().includes(needle)),
    [items, needle],
  );
  const folders = visible.filter((i) => i.is_dir);
  const files = visible.filter((i) => !i.is_dir);
  const kindFiles = loc.kind ? files.filter((f) => kindKey(f) === loc.kind) : files;
  // While searching, matches show as plain files, not inside kind tiles.
  const shownGrouping: Grouping = needle && grouping === "kind" ? "none" : grouping;

  // Searching also looks inside sub-folders (same words as the old Search
  // page: *.pdf, type:video, size:>10mb, modified:2026-09…), a moment after
  // typing stops.
  const [deep, setDeep] = useState<{ query: string; hits: BrowseItem[] } | null>(null);
  useEffect(() => {
    setDeep(null);
    if (needle.length < 2) return;
    let live = true;
    const t = window.setTimeout(() => {
      api
        .searchFiles(loc.path, filter.trim())
        .then((hits) => {
          if (!live) return;
          const here = new Set((items ?? []).map((i) => i.path));
          setDeep({
            query: needle,
            hits: hits
              .filter((h) => !here.has(h.path))
              .map((h) => ({ name: fileName(h.path), path: h.path, is_dir: false, size_bytes: h.size_bytes, modified: h.modified, category: h.category, children: null })),
          });
        })
        .catch(() => live && setDeep({ query: needle, hits: [] }));
    }, 300);
    return () => {
      live = false;
      window.clearTimeout(t);
    };
  }, [needle, loc.path, items]); // filter is needle untrimmed/uncased

  // Real system icons, and previews for pictures, for the files on screen.
  // Fetched once per path; the refs keep known ones out of the dependencies.
  const known = useRef({ icons, thumbs });
  known.current = { icons, thumbs };
  const onScreen = [...(loc.kind || shownGrouping !== "kind" ? kindFiles : []), ...(deep?.hits.slice(0, 120) ?? [])].map((f) => f.path);
  const screenKey = onScreen.join("|");
  useEffect(() => {
    const want = onScreen.filter((p) => !(p in known.current.icons)).slice(0, 240);
    const pics = settings.thumbnails
      ? onScreen.filter((p) => PICTURE.test(p) && !(p in known.current.thumbs)).slice(0, 240)
      : [];
    let live = true;
    (async () => {
      for (let i = 0; i < pics.length && live; i += 24) {
        const got = await api.thumbnails(pics.slice(i, i + 24)).catch(() => ({}) as Record<string, string>);
        if (live) setThumbs((m) => ({ ...m, ...got }));
      }
    })();
    (async () => {
      for (let i = 0; i < want.length && live; i += 24) {
        const batch = want.slice(i, i + 24).map((p) => [p, "file"] as [string, string]);
        const got = await api.searchIcons(batch).catch(() => ({}) as Record<string, string>);
        if (live) setIcons((m) => ({ ...m, ...got }));
      }
    })();
    return () => {
      live = false;
    };
  }, [screenKey, settings.thumbnails]); // onScreen is derived from screenKey

  const sections: { title: string; entries: Entry[] }[] = [];
  if (loc.kind) {
    if (shownGrouping === "date") pushDateSections(sections, kindFiles);
    else sections.push({ title: "", entries: kindFiles.map((item) => ({ type: "file", item })) });
  } else {
    if (folders.length) sections.push({ title: "Folders", entries: folders.map((item) => ({ type: "folder", item })) });
    if (shownGrouping === "kind") {
      const groups = new Map<KindKey, BrowseItem[]>();
      for (const f of files) groups.set(kindKey(f), [...(groups.get(kindKey(f)) ?? []), f]);
      const entries: Entry[] = KINDS.filter((k) => groups.has(k.key)).map((k) => ({ type: "kind", key: k.key, files: groups.get(k.key)! }));
      if (entries.length) sections.push({ title: "By kind", entries });
    } else if (shownGrouping === "date") {
      pushDateSections(sections, files);
    } else if (files.length) {
      sections.push({ title: "Files", entries: files.map((item) => ({ type: "file", item })) });
    }
  }
  const deepHits = deep && deep.query === needle ? deep.hits : [];
  if (deepHits.length) {
    sections.push({
      title: deepHits.length >= 2000 ? "In sub-folders (first 2000)" : "In sub-folders",
      entries: deepHits.map((item) => ({ type: "file", item })),
    });
  }
  const flat = sections.flatMap((s) => s.entries);
  /** Selected real files and folders (kind groups are only a view). */
  const targets = flat.filter((e): e is Exclude<Entry, { type: "kind" }> => e.type !== "kind" && selected.has(entryId(e))).map((e) => e.item);

  function open(e: Entry) {
    if (e.type === "folder") go({ path: e.item.path, kind: null });
    else if (e.type === "kind") go({ path: loc.path, kind: e.key });
    else api.launch(e.item.path).catch(toastError);
  }

  /** Click, Ctrl/Cmd+click (add or remove) and Shift+click (a range). */
  function select(e: Entry, ev: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }) {
    const id = entryId(e);
    const toggle = mac ? ev.metaKey : ev.ctrlKey;
    if (ev.shiftKey && anchor.current) {
      const a = flat.findIndex((x) => entryId(x) === anchor.current);
      const b = flat.findIndex((x) => entryId(x) === id);
      if (a >= 0 && b >= 0) {
        const [from, to] = a < b ? [a, b] : [b, a];
        setSelected(new Set(flat.slice(from, to + 1).map(entryId)));
        return;
      }
    }
    anchor.current = id;
    if (toggle) {
      setSelected((s) => {
        const next = new Set(s);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        return next;
      });
    } else setSelected(new Set([id]));
  }

  async function sortIntoFolders() {
    try {
      const byKind = new Map<KindKey, string[]>();
      for (const f of files) byKind.set(kindKey(f), [...(byKind.get(kindKey(f)) ?? []), f.path]);
      const plan: PlannedOp[] = [];
      for (const [key, paths] of byKind) plan.push(...(await api.planMoveInto(paths, joinPath(loc.path, kindOf(key).label.replace("&", "and")))));
      openPreview(plan);
    } catch (e) {
      toastError(e);
    }
  }

  async function newFolder() {
    try {
      const name = platform === "macos" ? "untitled folder" : platform === "windows" ? "New folder" : "New Folder";
      const made = await api.newFolder(loc.path, name);
      void record({ kind: "created", label: "New folder", paths: [made] });
      if (loc.kind) go({ path: loc.path, kind: null });
      refresh();
      setSelected(new Set([made]));
      setRenaming(made);
    } catch (e) {
      toastError(e);
    }
  }

  async function commitRename(item: BrowseItem, name: string) {
    setRenaming(null);
    root.current?.focus();
    if (!name.trim() || name === item.name) return;
    try {
      const to = await api.renameItem(item.path, name);
      void record({ kind: "rename", from: item.path, to });
      setSelected(new Set([to]));
      refresh();
    } catch (e) {
      toastError(e);
    }
  }

  /** Copy also goes on the system clipboard, so Explorer / Finder can paste
   *  it. Cut stays inside DeskZero: the clipboard can't mark files as cut. */
  function copyOrCut(mode: "copy" | "cut", list: BrowseItem[] = targets) {
    if (!list.length) return;
    clipStore.set({ mode, items: list.map((t) => ({ path: t.path, dir: t.is_dir })) });
    if (mode === "copy") void api.clipboardSetFiles(list.map((t) => t.path)).catch(toastError);
    toast(`${plural(list.length, "item")} ${mode === "copy" ? "copied" : "cut"}: paste them into another folder`);
  }

  async function copyInto(paths: string[], dest: string) {
    const r = await api.copyItems(paths, dest);
    if (r.done.length) void record({ kind: "created", label: "Copied", paths: r.done });
    report("copied", r);
  }

  /** Files and folders through the mover: History records it ("Moved"). */
  async function moveInto(list: { path: string; dir: boolean }[], dest: string) {
    const r = await api.moveItems(list.map((i) => i.path), dest);
    report("moved", { done: new Array<string>(r.moved).fill(dest), failed: r.failed.map((f) => [f.path, f.error] as [string, string]) });
  }


  /** DeskZero's own cut/copy first; otherwise files copied in Explorer or
   *  Finder (those are copied in, never moved). */
  async function paste() {
    const dest = loc.path;
    try {
      if (clip?.mode === "cut") {
        await moveInto(clip.items, dest);
        clipStore.set(null);
      } else {
        const outside = await api.clipboardGetFiles().catch(() => [] as string[]);
        const paths = outside.length ? outside : (clip?.items.map((i) => i.path) ?? []);
        if (!paths.length) return toast("Nothing to paste");
        await copyInto(paths, dest);
      }
    } catch (e) {
      toastError(e);
    }
    refresh();
  }

  function report(verb: string, r: Outcome) {
    if (r.done.length) toast(`${plural(r.done.length, "item")} ${verb} · ${mac ? "⌘" : "Ctrl+"}Z to undo`);
    for (const [path, err] of r.failed.slice(0, 3)) toast(`${fileName(path)}: ${err}`, true);
  }

  function askTrash(paths: string[]) {
    if (!paths.length) return;
    if (settings.confirm_delete) setConfirmTrash(paths);
    else void doTrash(paths);
  }

  async function doTrash(paths: string[]) {
    setConfirmTrash(null);
    try {
      await api.trashItems(paths);
      void record({ kind: "trashed", paths });
      toast(`${plural(paths.length, "item")} moved to the ${binName}`);
      setSelected(new Set());
    } catch (e) {
      toastError(e);
    }
    refresh();
    root.current?.focus();
  }

  async function copyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      toast("Path copied");
    } catch (e) {
      toastError(e);
    }
  }

  const pinned = settings.pinned ?? [];
  async function setPinned(path: string, on: boolean) {
    try {
      const cfg = await api.getConfig();
      const list = cfg.settings.pinned.filter((p) => p !== path);
      await api.saveSettings({ ...cfg.settings, pinned: on ? [...list, path] : list });
      await refreshConfig();
    } catch (e) {
      toastError(e);
    }
  }

  /** Space: Quick Look on a Mac, DeskZero's preview elsewhere. */
  async function peek(item: BrowseItem) {
    try {
      if (!(await api.quickLook(item.path))) setPeeking(item.path);
    } catch (e) {
      toastError(e);
    }
  }

  const revealLabel = platform === "macos" ? "Show in Finder" : platform === "windows" ? "Show in File Explorer" : "Show in Files";

  function itemMenu(e: Entry, x: number, y: number) {
    if (!selected.has(entryId(e))) {
      setSelected(new Set([entryId(e)]));
      anchor.current = entryId(e);
    }
    const many = selected.has(entryId(e)) ? targets : e.type === "kind" ? [] : [e.item];
    const one = e.type === "kind" ? null : e.item;
    const list: MenuItem[] = [{ label: "Open", accel: "Enter", run: () => open(e) }];
    if (one) {
      if (!one.is_dir) list.push({ label: "Preview", accel: "Space", run: () => void peek(one) });
      list.push({ label: revealLabel, run: () => api.revealFile(one.path).catch(toastError) });
      if (one.is_dir) {
        const isPinned = pinned.includes(one.path);
        list.push({ label: isPinned ? "Unpin from sidebar" : "Pin to sidebar", run: () => void setPinned(one.path, !isPinned) });
      }
      list.push(
        "-",
        { label: many.length > 1 ? `Rename ${many.length} items…` : "Rename", accel: "F2", run: () => (many.length > 1 ? setBatchRename(many.filter((m) => !m.is_dir).map((m) => m.path)) : setRenaming(one.path)) },
        { label: "Copy", accel: "Mod+C", run: () => copyOrCut("copy", many) },
        { label: "Cut", accel: "Mod+X", run: () => copyOrCut("cut", many) },
        { label: "Copy path", run: () => void copyPath(one.path) },
        "-",
        { label: `Move to ${binName}`, accel: mac ? "Mod+Backspace" : "Delete", danger: true, run: () => askTrash(many.map((m) => m.path)) },
      );
    }
    setMenu({ x, y, items: list });
  }

  function backgroundMenu(x: number, y: number) {
    setSelected(new Set());
    const isPinned = pinned.includes(loc.path);
    setMenu({
      x,
      y,
      items: [
        { label: "New folder", accel: "Mod+Shift+N", run: () => void newFolder() },
        { label: clip?.mode === "cut" ? `Paste ${plural(clip.items.length, "item")} here` : "Paste", accel: "Mod+V", run: () => void paste() },
        { label: "Undo", accel: "Mod+Z", disabled: !canUndo(), run: () => void undoLast() },
        "-",
        { label: "Select all", accel: "Mod+A", run: () => setSelected(new Set(flat.map(entryId))) },
        { label: "Clean up this folder…", run: () => setCleaning(true) },
        { label: isPinned ? "Unpin from sidebar" : "Pin to sidebar", run: () => void setPinned(loc.path, !isPinned) },
        { label: "Refresh", accel: "F5", run: refresh },
        "-",
        { label: revealLabel, run: () => api.openFolder(loc.path).catch(toastError) },
        { label: "Copy path", run: () => void copyPath(loc.path) },
      ],
    });
  }

  // Copy, cut and paste: on a Mac the system menu sends these as events,
  // elsewhere they arrive as keys (handled in onKey).
  useEffect(() => {
    if (!mac) return;
    const mine = () => root.current?.contains(document.activeElement) && !textFieldFocused();
    const onCopy = (ev: ClipboardEvent) => mine() && targets.length > 0 && (ev.preventDefault(), copyOrCut("copy"));
    const onCut = (ev: ClipboardEvent) => mine() && targets.length > 0 && (ev.preventDefault(), copyOrCut("cut"));
    const onPaste = (ev: ClipboardEvent) => mine() && (ev.preventDefault(), void paste());
    document.addEventListener("copy", onCopy);
    document.addEventListener("cut", onCut);
    document.addEventListener("paste", onPaste);
    return () => {
      document.removeEventListener("copy", onCopy);
      document.removeEventListener("cut", onCut);
      document.removeEventListener("paste", onPaste);
    };
  });

  // Files dragged in from Explorer / Finder: copied into the folder tile
  // they're dropped on, else into this folder.
  const dropRef = useRef<(paths: string[], x: number, y: number) => void>(() => undefined);
  dropRef.current = (paths, x, y) => {
    const hit = document.elementFromPoint(x / window.devicePixelRatio, y / window.devicePixelRatio);
    const dest = hit?.closest<HTMLElement>("[data-folder]")?.dataset.folder ?? loc.path;
    void copyInto(paths, dest).then(refresh, toastError);
  };
  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "enter" || p.type === "over") setDropping(true);
      else if (p.type === "leave") setDropping(false);
      else if (p.type === "drop") {
        setDropping(false);
        dropRef.current(p.paths, p.position.x, p.position.y);
      }
    });
    return () => void un.then((f) => f());
  }, []);

  // Dragging tiles onto a folder tile inside DeskZero moves them (undoable).
  // Done with mouse events: the window's file drop (above) turns off the
  // browser's own drag and drop on Windows.
  const [drag, setDrag] = useState<{ x: number; y: number; count: number; over: string | null } | null>(null);
  function startDrag(e: Entry, ev: React.MouseEvent) {
    if (ev.button !== 0 || e.type === "kind" || renaming) return;
    const sx = ev.clientX;
    const sy = ev.clientY;
    const id = entryId(e);
    const moving = selected.has(id) ? targets : [e.item];
    let active = false;
    const overFolder = (x: number, y: number) => {
      const dest = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-folder]")?.dataset.folder ?? null;
      return dest && !moving.some((m) => m.path === dest) ? dest : null;
    };
    const move = (m: MouseEvent) => {
      if (!active && Math.hypot(m.clientX - sx, m.clientY - sy) < 6) return;
      active = true;
      setDrag({ x: m.clientX, y: m.clientY, count: moving.length, over: overFolder(m.clientX, m.clientY) });
    };
    const up = (m: MouseEvent) => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      setDrag(null);
      if (!active) return;
      const dest = overFolder(m.clientX, m.clientY);
      if (dest) void moveInto(moving.map((t) => ({ path: t.path, dir: t.is_dir })), dest).then(refresh, toastError);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  function onKey(e: React.KeyboardEvent) {
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    const mod = mac ? e.metaKey : e.ctrlKey;
    const key = e.key.toLowerCase();
    const handled = () => (e.preventDefault(), e.stopPropagation());
    if (mod && e.shiftKey && key === "n") return void (handled(), newFolder());
    if (mod && !mac && key === "c") return void (handled(), copyOrCut("copy"));
    if (mod && !mac && key === "x") return void (handled(), copyOrCut("cut"));
    if (mod && !mac && key === "v") return void (handled(), paste());
    if (mod && !e.shiftKey && key === "z") return void (handled(), undoLast());
    if (mod && key === "a") return void (handled(), setSelected(new Set(flat.map(entryId))));
    if (e.key === "F5") return void (handled(), refresh());
    if (e.key === "F2" && targets.length === 1) return void (handled(), setRenaming(targets[0].path));
    if (e.key === "F2" && targets.length > 1) return void (handled(), setBatchRename(targets.filter((t) => !t.is_dir).map((t) => t.path)));
    if (e.key === " " && targets.length === 1 && !targets[0].is_dir) return void (handled(), peek(targets[0]));
    if ((e.key === "Delete" || (mac && mod && e.key === "Backspace")) && targets.length) {
      return void (handled(), askTrash(targets.map((t) => t.path)));
    }
    if (e.key === "Escape") return void (handled(), setSelected(new Set()));
    if (e.key === "Backspace" || (e.altKey && e.key === "ArrowLeft") || (mac && e.metaKey && e.key === "[")) return void (handled(), back());
    if ((e.altKey && e.key === "ArrowRight") || (mac && e.metaKey && e.key === "]")) return void (handled(), forward());
    if ((e.altKey || (mac && e.metaKey)) && e.key === "ArrowUp") return void (handled(), goUp());
    const current = flat.findIndex((x) => selected.has(entryId(x)));
    if (e.key === "Enter" && current >= 0) return void (handled(), open(flat[current]));
    if (["ArrowRight", "ArrowLeft", "ArrowDown", "ArrowUp"].includes(e.key) && flat.length) {
      handled();
      const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : -1;
      const next = flat[Math.max(0, Math.min(flat.length - 1, current < 0 ? 0 : current + step))];
      anchor.current = entryId(next);
      setSelected(new Set([entryId(next)]));
    }
  }

  function goUp() {
    if (loc.kind) return go({ path: loc.path, kind: null });
    const up = parentOf(loc.path);
    if (up) go({ path: up, kind: null });
  }

  const trail = crumbs(loc.path);
  // The status line counts what is on screen: one kind, or the whole folder.
  const shown = loc.kind ? kindFiles.length : visible.length;
  const fileBytes = kindFiles.reduce((n, f) => n + f.size_bytes, 0);
  const itemProps = (e: Entry) => ({
    entry: e,
    size: e.type === "folder" ? sizes[e.item.path] : undefined,
    icon: e.type === "file" ? icons[e.item.path] : undefined,
    thumb: e.type === "file" ? thumbs[e.item.path] : undefined,
    selected: selected.has(entryId(e)),
    cut: e.type !== "kind" && clip?.mode === "cut" && clip.items.some((i) => i.path === e.item.path),
    renaming: e.type !== "kind" && renaming === e.item.path,
    onSelect: (ev: React.MouseEvent) => select(e, ev),
    onMouseDown: (ev: React.MouseEvent) => startDrag(e, ev),
    dropTarget: e.type === "folder" && drag?.over === e.item.path,
    onOpen: () => open(e),
    onMenu: (ev: React.MouseEvent) => (ev.preventDefault(), ev.stopPropagation(), itemMenu(e, ev.clientX, ev.clientY)),
    onRename: (name: string) => e.type !== "kind" && void commitRename(e.item, name),
    onCancelRename: () => (setRenaming(null), root.current?.focus()),
  });

  return (
    <div className="browser" tabIndex={-1} onKeyDown={onKey} ref={root}>
      <div className="bz-toolbar">
        <div className="bz-nav">
          <button className="bz-icon" onClick={back} disabled={!canBack} title="Back (Alt+←)" aria-label="Back"><BackIcon size={17} /></button>
          <button className="bz-icon" onClick={forward} disabled={!canForward} title="Forward (Alt+→)" aria-label="Forward"><ForwardIcon size={17} /></button>
          <button className="bz-icon" onClick={goUp} disabled={!loc.kind && !parentOf(loc.path)} title="Up (Alt+↑)" aria-label="Up one level"><UpIcon size={16} /></button>
          <button className="bz-icon" onClick={refresh} title="Refresh (F5)" aria-label="Refresh"><RefreshIcon size={16} /></button>
          <button className="bz-icon" onClick={() => void newFolder()} title="New folder" aria-label="New folder"><NewFolderIcon size={17} /></button>
          <button className="bz-icon" onClick={() => setCleaning(true)} title="Clean up this folder" aria-label="Clean up this folder"><BroomIcon size={16} /></button>
        </div>
        <nav className="bz-crumbs" aria-label="Folder path" title={loc.path}>
          {trail.map((c, i) => (
            <span key={c.path} className="bz-crumb">
              {i > 0 && <span className="bz-sep">›</span>}
              <button
                className={i === trail.length - 1 && !loc.kind ? "current" : ""}
                onClick={() => go({ path: c.path, kind: null })}
              >
                {c.label}
              </button>
            </span>
          ))}
          {loc.kind && (
            <span className="bz-crumb">
              <span className="bz-sep">›</span>
              <button className="current">{kindOf(loc.kind).label}</button>
            </span>
          )}
        </nav>
        <div className="bz-tools">
          <div className="segmented small-seg" role="group" aria-label="Group by">
            {(["none", "kind", "date"] as const).map((g) => (
              <button
                key={g}
                className={grouping === g ? "active" : ""}
                aria-pressed={grouping === g}
                onClick={() => (setGrouping(g), savePref("dz.grouping", g))}
              >
                {g === "none" ? "None" : g === "kind" ? "Kind" : "Date"}
              </button>
            ))}
          </div>
          <div className="segmented small-seg" role="group" aria-label="View">
            <button className={view === "list" ? "active" : ""} aria-pressed={view === "list"} title="List" aria-label="List view" onClick={() => (setView("list"), savePref("dz.view", "list"))}>
              <ListIcon size={15} />
            </button>
            <button className={view === "grid" ? "active" : ""} aria-pressed={view === "grid"} title="Icons" aria-label="Icon view" onClick={() => (setView("grid"), savePref("dz.view", "grid"))}>
              <AppGridIcon size={15} />
            </button>
          </div>
          <label className="bz-filter">
            <SearchIcon size={14} />
            <input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder={`Search ${loc.kind ? kindOf(loc.kind).label : trail[trail.length - 1]?.label ?? "folder"}`}
              aria-label="Search this folder and its sub-folders"
              title="Name, *.pdf, type:video, size:>10mb, modified:2026-09"
              spellCheck={false}
            />
          </label>
        </div>
      </div>

      <div
        className="bz-body"
        onClick={(e) => e.target === e.currentTarget && setSelected(new Set())}
        onContextMenu={(e) => (e.preventDefault(), backgroundMenu(e.clientX, e.clientY))}
      >
        {error && <p className="error-text bz-msg">{error}</p>}
        {!error && items && flat.length === 0 && (
          <p className="muted bz-msg">{needle ? (deep ? "Nothing here or in sub-folders matches." : "Searching…") : "This folder is empty."}</p>
        )}
        {!loc.kind && shownGrouping === "kind" && files.length > 1 && (
          <div className="bz-hint">
            <SparkleIcon size={15} />
            <span>These groups are just a view. Want real folders?</span>
            <button className="link" onClick={sortIntoFolders}>Sort into folders…</button>
          </div>
        )}
        {sections.map((s) => (
          <section key={s.title || "all"} className="bz-section">
            {s.title && (
              <h2 className="bz-title">
                {s.title} <span>{s.entries.length}</span>
              </h2>
            )}
            {view === "grid" ? (
              <div className="bz-grid">
                {s.entries.map((e) => <Tile key={entryId(e)} {...itemProps(e)} />)}
              </div>
            ) : (
              <div className="bz-list" role="list">
                {s.entries.map((e) => <Row key={entryId(e)} {...itemProps(e)} />)}
              </div>
            )}
          </section>
        ))}
      </div>

      <footer className="bz-status">
        <span>
          {targets.length > 1
            ? `${targets.length} of ${shown} selected`
            : `${plural(shown, "item")}${kindFiles.length > 0 ? ` · ${humanSize(fileBytes)} in files` : ""}`}
          {space && ` · ${humanSize(space[1])} free`}
        </span>
        <button className="link" onClick={() => api.openFolder(loc.path).catch(toastError)}>{revealLabel}</button>
      </footer>

      {menu && <ContextMenu {...menu} onClose={() => setMenu(null)} />}
      {dropping && <div className="bz-dropzone">Drop to copy here</div>}
      {drag && (
        <div className="bz-drag-ghost" style={{ left: drag.x + 12, top: drag.y + 12 }}>
          {drag.over ? `Move ${plural(drag.count, "item")} to ${fileName(drag.over)}` : plural(drag.count, "item")}
        </div>
      )}
      {peeking && (
        <PreviewSheet
          files={kindFiles}
          path={peeking}
          onPath={(p) => (setPeeking(p), setSelected(new Set([p])))}
          onClose={() => (setPeeking(null), root.current?.focus())}
          onOpen={(p) => api.launch(p).catch(toastError)}
        />
      )}
      {batchRename && (
        <RenameSheet
          paths={batchRename}
          onClose={() => (setBatchRename(null), root.current?.focus())}
          onDone={() => (setBatchRename(null), refresh())}
        />
      )}
      {cleaning && (
        <CleanupSheet
          dir={loc.path}
          binName={binName}
          onClose={() => (setCleaning(false), root.current?.focus())}
          onTrash={(paths) => (setCleaning(false), void doTrash(paths))}
        />
      )}
      {confirmTrash && (
        <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={`Move to ${binName}`} onMouseDown={(e) => e.target === e.currentTarget && setConfirmTrash(null)}>
          <div className="modal confirm-card">
            <h2>
              Move {confirmTrash.length === 1 ? `"${fileName(confirmTrash[0])}"` : plural(confirmTrash.length, "item")} to the {binName}?
            </h2>
            <p className="muted">You can put {confirmTrash.length === 1 ? "it" : "them"} back from the {binName}. Turn this question off in Settings › Editing.</p>
            <div className="modal-actions">
              <button onClick={() => setConfirmTrash(null)}>Cancel</button>
              <button className="primary" autoFocus onClick={() => void doTrash(confirmTrash)}>
                Move to {binName}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/** Files the thumbnail command can make a preview of. */
const PICTURE = /\.(jpe?g|jfif|png|gif|webp|bmp|ico|tiff?|svg)$/i;

function pushDateSections(sections: { title: string; entries: Entry[] }[], files: BrowseItem[]) {
  const now = new Date();
  const groups = new Map<string, BrowseItem[]>();
  for (const f of files) {
    const g = dateGroup(f.modified, now);
    groups.set(g, [...(groups.get(g) ?? []), f]);
  }
  for (const title of DATE_GROUPS) {
    const list = groups.get(title);
    if (!list) continue;
    list.sort((a, b) => (b.modified ?? "").localeCompare(a.modified ?? ""));
    sections.push({ title, entries: list.map((item) => ({ type: "file", item })) });
  }
}

function describe(e: Entry, size: number | undefined): { name: string; count: string; bytes: string; title: string } {
  if (e.type === "folder") {
    const n = e.item.children;
    return {
      name: e.item.name,
      count: n == null ? "No access" : `${n} item${n === 1 ? "" : "s"}`,
      bytes: size == null ? "" : humanSize(size),
      title: e.item.path,
    };
  }
  if (e.type === "kind") {
    const n = e.files.length;
    const k = kindOf(e.key);
    return {
      name: k.label,
      count: `${n} file${n === 1 ? "" : "s"}`,
      bytes: humanSize(e.files.reduce((s, f) => s + f.size_bytes, 0)),
      title: `${n} ${k.label.toLowerCase()} in this folder`,
    };
  }
  return { name: e.item.name, count: "", bytes: humanSize(e.item.size_bytes), title: `${e.item.name}\n${dateTime(e.item.modified)}` };
}

interface ItemProps {
  entry: Entry;
  size?: number;
  icon?: string;
  thumb?: string;
  selected: boolean;
  cut: boolean;
  renaming: boolean;
  onSelect: (e: React.MouseEvent) => void;
  onOpen: () => void;
  onMenu: (e: React.MouseEvent) => void;
  onMouseDown: (e: React.MouseEvent) => void;
  dropTarget: boolean;
  onRename: (name: string) => void;
  onCancelRename: () => void;
}

/** The name field shown while renaming: the name without its extension is
 *  pre-selected, like Explorer and Finder. Enter saves, Esc cancels. */
function RenameField({ initial, onRename, onCancel }: { initial: string; onRename: (n: string) => void; onCancel: () => void }) {
  const ref = useRef<HTMLInputElement>(null);
  const done = useRef(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.focus();
    const dot = initial.lastIndexOf(".");
    el.setSelectionRange(0, dot > 0 ? dot : initial.length);
  }, [initial]);
  const finish = (save: boolean) => {
    if (done.current) return;
    done.current = true;
    if (save) onRename(ref.current?.value ?? initial);
    else onCancel();
  };
  return (
    <input
      ref={ref}
      className="bz-rename"
      defaultValue={initial}
      spellCheck={false}
      aria-label="New name"
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") finish(true);
        else if (e.key === "Escape") finish(false);
      }}
      onBlur={() => finish(true)}
    />
  );
}

function Tile({ entry, size, icon, thumb, selected, cut, renaming, onSelect, onOpen, onMenu, onMouseDown, dropTarget, onRename, onCancelRename }: ItemProps) {
  const d = describe(entry, size);
  return (
    <div
      role="button"
      tabIndex={0}
      className={`bz-tile ${entry.type}${selected ? " selected" : ""}${cut ? " cut" : ""}${dropTarget ? " drop" : ""}`}
      data-folder={entry.type === "folder" ? entry.item.path : undefined}
      onMouseDown={onMouseDown}
      title={renaming ? undefined : d.title}
      onClick={onSelect}
      onDoubleClick={onOpen}
      onContextMenu={onMenu}
    >
      {entry.type === "file" ? (
        <FileGlyph item={entry.item} icon={icon} thumb={thumb} />
      ) : (
        <FolderArt kind={entry.type === "kind" ? entry.key : null} count={d.count} bytes={d.bytes} />
      )}
      {renaming ? (
        <RenameField initial={d.name} onRename={onRename} onCancel={onCancelRename} />
      ) : (
        <span className="bz-name">{d.name}</span>
      )}
      {entry.type === "file" && !renaming && <span className="bz-meta">{d.bytes}</span>}
    </div>
  );
}

function Row({ entry, size, icon, thumb, selected, cut, renaming, onSelect, onOpen, onMenu, onMouseDown, dropTarget, onRename, onCancelRename }: ItemProps) {
  const d = describe(entry, size);
  const kind = entry.type === "kind" ? kindOf(entry.key) : null;
  return (
    <div
      role="listitem"
      tabIndex={0}
      className={`bz-row${selected ? " selected" : ""}${cut ? " cut" : ""}${dropTarget ? " drop" : ""}`}
      data-folder={entry.type === "folder" ? entry.item.path : undefined}
      onMouseDown={onMouseDown}
      title={renaming ? undefined : d.title}
      onClick={onSelect}
      onDoubleClick={onOpen}
      onContextMenu={onMenu}
    >
      <span className="bz-row-icon">
        {entry.type === "file" ? (
          <FileGlyph item={entry.item} icon={icon} thumb={thumb} small />
        ) : (
          <span className={`bz-mini-folder${kind ? "" : " plain"}`} style={kind ? ({ "--tile": kind.color } as React.CSSProperties) : undefined} />
        )}
      </span>
      {renaming ? (
        <RenameField initial={d.name} onRename={onRename} onCancel={onCancelRename} />
      ) : (
        <span className="bz-row-name">{d.name}</span>
      )}
      <span className="bz-row-kind">{entry.type === "file" ? kindOf(kindKey(entry.item)).label : entry.type === "kind" ? d.count : "Folder"}</span>
      <span className="bz-row-date">{entry.type === "kind" ? "" : dateTime(entry.item.modified)}</span>
      <span className="bz-row-size">{entry.type === "folder" ? d.bytes || "—" : d.bytes}</span>
    </div>
  );
}

function FileGlyph({ item, icon, thumb, small }: { item: BrowseItem; icon?: string; thumb?: string; small?: boolean }) {
  const k = kindOf(kindKey(item));
  if (thumb) return <img className={small ? "bz-thumb small" : "bz-thumb"} src={thumb} alt="" draggable={false} />;
  if (icon) return <img className={small ? "bz-sysicon small" : "bz-sysicon"} src={icon} alt="" draggable={false} />;
  const Icon = k.Icon;
  return (
    <span className={small ? "bz-glyph small" : "bz-glyph"} style={{ "--tile": k.color } as React.CSSProperties}>
      <Icon size={small ? 13 : 26} />
    </span>
  );
}

/** The folder picture: a tab and back panel, a few sheets of paper, and a
 *  glassy front panel with the count, size and kind icon on it. */
function FolderArt({ kind, count, bytes }: { kind: KindKey | null; count: string; bytes: string }) {
  const id = useId().replace(/:/g, "");
  const k = kind ? kindOf(kind) : null;
  const Icon = k?.Icon;
  return (
    <span className={k ? "bz-folder" : "bz-folder plain"} style={k ? ({ "--tile": k.color } as React.CSSProperties) : undefined}>
      <svg viewBox="0 0 120 92" aria-hidden="true">
        <defs>
          <linearGradient id={`f${id}`} x1="0" y1="0" x2="0.35" y2="1">
            <stop offset="0" className="bz-front-a" />
            <stop offset="1" className="bz-front-b" />
          </linearGradient>
        </defs>
        <path className="bz-back" d="M4 10a7 7 0 0 1 7-7h28a7 7 0 0 1 5 2.1L50 11h59a7 7 0 0 1 7 7v62a7 7 0 0 1-7 7H11a7 7 0 0 1-7-7Z" />
        <rect className="bz-paper" x="15" y="15" width="84" height="40" rx="3" transform="rotate(-4 57 35)" />
        <rect className="bz-paper" x="20" y="17" width="84" height="40" rx="3" transform="rotate(2.5 62 37)" />
        <rect x="4" y="30" width="112" height="59" rx="8" fill={`url(#f${id})`} />
        <rect className="bz-edge" x="4.5" y="30.5" width="111" height="58" rx="7.5" />
      </svg>
      {k && <span className="bz-fcount">{count}</span>}
      {Icon && <span className="bz-ficon"><Icon size={26} /></span>}
      {k && <span className="bz-fbytes">{bytes}</span>}
      {!k && (count || bytes) && (
        <span className="bz-fplain">
          {count}
          {bytes && <em>{bytes}</em>}
        </span>
      )}
    </span>
  );
}
