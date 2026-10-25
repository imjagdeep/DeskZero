// The file browser DeskZero opens on: a folder (Downloads by default) with
// its sub-folders, and its loose files grouped by kind or date. Grouping is
// only a view: nothing moves until "Sort into folders", which goes through
// the usual preview and can be undone.

import { useEffect, useId, useMemo, useRef, useState } from "react";
import {
  AppGridIcon,
  ArchiveIcon,
  BackIcon,
  CodeIcon,
  DiscIcon,
  DocumentIcon,
  FilmIcon,
  ForwardIcon,
  ImageIcon,
  ListIcon,
  MoreIcon,
  MusicIcon,
  PackageIcon,
  PdfIcon,
  RefreshIcon,
  SearchIcon,
  SlidesIcon,
  SparkleIcon,
  TableIcon,
  UpIcon,
} from "../components/Icons";
import { api } from "../lib/api";
import { dateTime, humanSize } from "../lib/format";
import { openPreview, toastError, usePreview } from "../lib/store";
import type { BrowseItem, Category, PlannedOp } from "../lib/types";

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

export function Browse({
  loc,
  go,
  back,
  forward,
  canBack,
  canForward,
  platform,
}: {
  loc: Location;
  go: (l: Location) => void;
  back: () => void;
  forward: () => void;
  canBack: boolean;
  canForward: boolean;
  platform: string;
}) {
  const [items, setItems] = useState<BrowseItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sizes, setSizes] = useState<Record<string, number>>({});
  const [icons, setIcons] = useState<Record<string, string>>({});
  const [grouping, setGrouping] = useState<Grouping>(() => pref("dz.grouping", "kind", ["none", "kind", "date"] as const));
  const [view, setView] = useState<View>(() => pref("dz.view", "grid", ["grid", "list"] as const));
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const preview = usePreview();
  const hadPreview = useRef(false);

  // After "Sort into folders" (or any organize) closes, show the result.
  useEffect(() => {
    if (preview) hadPreview.current = true;
    else if (hadPreview.current) {
      hadPreview.current = false;
      setReload((n) => n + 1);
    }
  }, [preview]);

  useEffect(() => {
    let live = true;
    setError(null);
    setSelected(null);
    api
      .listFolder(loc.path, false)
      .then((list) => {
        if (!live) return;
        setItems(list);
        const dirs = list.filter((i) => i.is_dir).map((i) => i.path);
        if (dirs.length) {
          api
            .folderSizes(dirs)
            .then((pairs) => live && setSizes(Object.fromEntries(pairs)))
            .catch(() => undefined); // sizes are a nicety; tiles still show counts
        }
      })
      .catch((e) => {
        if (!live) return;
        setItems([]);
        setError(typeof e === "string" ? e : String(e));
      });
    return () => {
      live = false;
    };
  }, [loc.path, reload]);

  useEffect(() => setFilter(""), [loc.path, loc.kind]);

  const needle = filter.trim().toLowerCase();
  const visible = useMemo(
    () => (items ?? []).filter((i) => !needle || i.name.toLowerCase().includes(needle)),
    [items, needle],
  );
  const folders = visible.filter((i) => i.is_dir);
  const files = visible.filter((i) => !i.is_dir);
  const kindFiles = loc.kind ? files.filter((f) => kindKey(f) === loc.kind) : files;

  // Real system icons for the files on screen (Windows and macOS). Fetched
  // once per path; the ref keeps already-known icons out of the dependencies.
  const known = useRef(icons);
  known.current = icons;
  const iconPaths = (loc.kind || grouping !== "kind" ? kindFiles : []).map((f) => f.path);
  const iconKey = iconPaths.join("|");
  useEffect(() => {
    const want = iconPaths.filter((p) => !(p in known.current)).slice(0, 240);
    if (!want.length) return;
    let live = true;
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
  }, [iconKey]); // iconPaths is derived from iconKey

  const sections: { title: string; entries: Entry[] }[] = [];
  if (loc.kind) {
    if (grouping === "date") pushDateSections(sections, kindFiles);
    else sections.push({ title: "", entries: kindFiles.map((item) => ({ type: "file", item })) });
  } else {
    if (folders.length) sections.push({ title: "Folders", entries: folders.map((item) => ({ type: "folder", item })) });
    if (grouping === "kind") {
      const groups = new Map<KindKey, BrowseItem[]>();
      for (const f of files) groups.set(kindKey(f), [...(groups.get(kindKey(f)) ?? []), f]);
      const entries: Entry[] = KINDS.filter((k) => groups.has(k.key)).map((k) => ({ type: "kind", key: k.key, files: groups.get(k.key)! }));
      if (entries.length) sections.push({ title: "By kind", entries });
    } else if (grouping === "date") {
      pushDateSections(sections, files);
    } else if (files.length) {
      sections.push({ title: "Files", entries: files.map((item) => ({ type: "file", item })) });
    }
  }

  function open(e: Entry) {
    if (e.type === "folder") go({ path: e.item.path, kind: null });
    else if (e.type === "kind") go({ path: loc.path, kind: e.key });
    else api.launch(e.item.path).catch(toastError);
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

  const flat = sections.flatMap((s) => s.entries);
  function onKey(e: React.KeyboardEvent) {
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    if (e.key === "Backspace" || (e.altKey && e.key === "ArrowLeft")) return void (e.preventDefault(), back());
    if (e.altKey && e.key === "ArrowRight") return void (e.preventDefault(), forward());
    if (e.altKey && e.key === "ArrowUp") return void (e.preventDefault(), goUp());
    if (e.key === "Enter" && selected) {
      const hit = flat.find((x) => entryId(x) === selected);
      if (hit) open(hit);
      return;
    }
    if (["ArrowRight", "ArrowLeft", "ArrowDown", "ArrowUp"].includes(e.key) && flat.length) {
      e.preventDefault();
      const i = flat.findIndex((x) => entryId(x) === selected);
      const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : -1;
      const next = flat[Math.max(0, Math.min(flat.length - 1, i < 0 ? 0 : i + step))];
      setSelected(entryId(next));
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
  const revealLabel = platform === "macos" ? "Show in Finder" : platform === "windows" ? "Open in File Explorer" : "Open in Files";

  return (
    <div className="browser" tabIndex={-1} onKeyDown={onKey}>
      <div className="bz-toolbar">
        <div className="bz-nav">
          <button className="bz-icon" onClick={back} disabled={!canBack} title="Back (Alt+←)" aria-label="Back"><BackIcon size={17} /></button>
          <button className="bz-icon" onClick={forward} disabled={!canForward} title="Forward (Alt+→)" aria-label="Forward"><ForwardIcon size={17} /></button>
          <button className="bz-icon" onClick={goUp} disabled={!loc.kind && !parentOf(loc.path)} title="Up (Alt+↑)" aria-label="Up one level"><UpIcon size={16} /></button>
          <button className="bz-icon" onClick={() => setReload((n) => n + 1)} title="Refresh" aria-label="Refresh"><RefreshIcon size={16} /></button>
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
              placeholder={`Filter ${loc.kind ? kindOf(loc.kind).label : trail[trail.length - 1]?.label ?? "folder"}`}
              aria-label="Filter this folder"
              spellCheck={false}
            />
          </label>
        </div>
      </div>

      <div className="bz-body" onClick={(e) => e.target === e.currentTarget && setSelected(null)}>
        {error && <p className="error-text bz-msg">{error}</p>}
        {!error && items && flat.length === 0 && (
          <p className="muted bz-msg">{needle ? "Nothing here matches the filter." : "This folder is empty."}</p>
        )}
        {!loc.kind && grouping === "kind" && files.length > 1 && (
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
                {s.entries.map((e) => (
                  <Tile
                    key={entryId(e)}
                    entry={e}
                    size={e.type === "folder" ? sizes[e.item.path] : undefined}
                    icon={e.type === "file" ? icons[e.item.path] : undefined}
                    selected={selected === entryId(e)}
                    onSelect={() => setSelected(entryId(e))}
                    onOpen={() => open(e)}
                  />
                ))}
              </div>
            ) : (
              <div className="bz-list" role="list">
                {s.entries.map((e) => (
                  <Row
                    key={entryId(e)}
                    entry={e}
                    size={e.type === "folder" ? sizes[e.item.path] : undefined}
                    icon={e.type === "file" ? icons[e.item.path] : undefined}
                    selected={selected === entryId(e)}
                    onSelect={() => setSelected(entryId(e))}
                    onOpen={() => open(e)}
                  />
                ))}
              </div>
            )}
          </section>
        ))}
      </div>

      <footer className="bz-status">
        <span>
          {shown} item{shown === 1 ? "" : "s"}
          {kindFiles.length > 0 && ` · ${humanSize(fileBytes)} in files`}
        </span>
        <button className="link" onClick={() => api.openFolder(loc.path).catch(toastError)}>{revealLabel}</button>
      </footer>
    </div>
  );
}

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
  selected: boolean;
  onSelect: () => void;
  onOpen: () => void;
}

function Tile({ entry, size, icon, selected, onSelect, onOpen }: ItemProps) {
  const d = describe(entry, size);
  return (
    <button
      className={`bz-tile ${entry.type}${selected ? " selected" : ""}`}
      title={d.title}
      onClick={onSelect}
      onDoubleClick={onOpen}
      onFocus={onSelect}
    >
      {entry.type === "file" ? (
        <FileGlyph item={entry.item} icon={icon} />
      ) : (
        <FolderArt
          kind={entry.type === "kind" ? entry.key : null}
          count={d.count}
          bytes={d.bytes}
        />
      )}
      <span className="bz-name">{d.name}</span>
      {entry.type === "file" && <span className="bz-meta">{d.bytes}</span>}
    </button>
  );
}

function Row({ entry, size, icon, selected, onSelect, onOpen }: ItemProps) {
  const d = describe(entry, size);
  const kind = entry.type === "kind" ? kindOf(entry.key) : null;
  return (
    <button
      role="listitem"
      className={`bz-row${selected ? " selected" : ""}`}
      title={d.title}
      onClick={onSelect}
      onDoubleClick={onOpen}
      onFocus={onSelect}
    >
      <span className="bz-row-icon">
        {entry.type === "file" ? (
          <FileGlyph item={entry.item} icon={icon} small />
        ) : (
          <span className={`bz-mini-folder${kind ? "" : " plain"}`} style={kind ? ({ "--tile": kind.color } as React.CSSProperties) : undefined} />
        )}
      </span>
      <span className="bz-row-name">{d.name}</span>
      <span className="bz-row-kind">{entry.type === "file" ? kindOf(kindKey(entry.item)).label : entry.type === "kind" ? d.count : "Folder"}</span>
      <span className="bz-row-date">{entry.type === "kind" ? "" : dateTime(entry.item.modified)}</span>
      <span className="bz-row-size">{entry.type === "folder" ? d.bytes || "—" : d.bytes}</span>
    </button>
  );
}

function FileGlyph({ item, icon, small }: { item: BrowseItem; icon?: string; small?: boolean }) {
  const k = kindOf(kindKey(item));
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
