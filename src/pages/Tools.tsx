// Search, Duplicates and Rename: read-only tools (rename applies only on request).

import { ask } from "@tauri-apps/plugin-dialog";
import { useState, useSyncExternalStore } from "react";
import { EmptyState, FolderField, PageHeader, RevealButton } from "../components/Common";
import { CheckCircleIcon } from "../components/Icons";
import { api } from "../lib/api";
import { categoryLabel, dateTime, fileName, humanSize, parentDir } from "../lib/format";
import { createStore, openPreview, toast, toastError, useConfig } from "../lib/store";
import type { DupGroup, FileInfo, RenameRow } from "../lib/types";

function useStartFolder(): [string, (p: string) => void] {
  const cfg = useConfig();
  const [folder, setFolder] = useState<string | null>(null);
  return [folder ?? cfg?.settings.organize_root ?? "", setFolder];
}

// Results live outside the pages so switching pages (even mid-search) keeps them.
const searchStore = createStore<{ query: string; hits: FileInfo[] | null; busy: boolean }>({
  query: "",
  hits: null,
  busy: false,
});
const dupStore = createStore<{ groups: DupGroup[] | null; busy: boolean }>({ groups: null, busy: false });

const EXAMPLES = ["invoice", "*.pdf", "large files", "type:video", "size:>10mb", "modified:2026-09", "folder:scans"];

export function Search() {
  const [folder, setFolder] = useStartFolder();
  const { query, hits, busy } = useSyncExternalStore(searchStore.subscribe, searchStore.get);
  const setQuery = (q: string) => searchStore.set({ ...searchStore.get(), query: q });

  async function run(q = query) {
    if (!folder || !q.trim()) return;
    searchStore.set({ ...searchStore.get(), busy: true });
    try {
      searchStore.set({ ...searchStore.get(), hits: await api.searchFiles(folder, q), busy: false });
    } catch (e) {
      searchStore.set({ ...searchStore.get(), busy: false });
      toastError(e);
    }
  }

  return (
    <div className="page">
      <PageHeader title="Search" />
      <FolderField label="Look in" value={folder} onChange={setFolder} />
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          void run();
        }}
      >
        <input className="grow" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="invoice, *.pdf, large files…" autoFocus />
        <button className="primary" disabled={busy || !folder}>{busy ? "Searching…" : "Search"}</button>
      </form>
      <div className="chips">
        {EXAMPLES.map((ex) => (
          <button key={ex} className="chip" onClick={() => { setQuery(ex); void run(ex); }}>{ex}</button>
        ))}
      </div>
      {hits && (
        <>
          <p className="muted">{hits.length === 2000 ? "First 2000 results" : `${hits.length} result${hits.length === 1 ? "" : "s"}`}</p>
          <table className="table fade-in">
            <thead>
              <tr><th>Name</th><th>Folder</th><th>Type</th><th className="num">Size</th><th>Modified</th><th /></tr>
            </thead>
            <tbody>
              {hits.map((h) => (
                <tr key={h.path}>
                  <td>{fileName(h.path)}</td>
                  <td className="muted ellipsis" title={parentDir(h.path)}>{parentDir(h.path)}</td>
                  <td>{h.category ? categoryLabel(h.category) : ""}</td>
                  <td className="num">{humanSize(h.size_bytes)}</td>
                  <td>{dateTime(h.modified)}</td>
                  <td><RevealButton path={h.path} /></td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </div>
  );
}

export function Duplicates() {
  const [folder, setFolder] = useStartFolder();
  const { groups, busy } = useSyncExternalStore(dupStore.subscribe, dupStore.get);

  async function scan() {
    dupStore.set({ groups: dupStore.get().groups, busy: true });
    try {
      dupStore.set({ groups: await api.findDuplicates(folder), busy: false });
    } catch (e) {
      dupStore.set({ groups: dupStore.get().groups, busy: false });
      toastError(e);
    }
  }

  const wasted = groups?.reduce((n, g) => n + g.size_bytes * (g.files.length - 1), 0) ?? 0;
  const dupDir = folder ? folder.replace(/[\\/]+$/, "") + (folder.includes("\\") ? "\\" : "/") + "Duplicates" : "";

  /** Keep the oldest copy (usually the original); move the rest aside. */
  function extras(gs: DupGroup[]): string[] {
    return gs.flatMap((g) => {
      const sorted = [...g.files].sort((a, b) => (a.modified ?? "").localeCompare(b.modified ?? ""));
      return sorted.slice(1).map((f) => f.path);
    });
  }

  async function moveAside(gs: DupGroup[]) {
    try {
      openPreview(await api.planMoveInto(extras(gs), dupDir));
    } catch (e) {
      toastError(e);
    }
  }

  return (
    <div className="page">
      <PageHeader title="Duplicates">
        <button className="primary" onClick={scan} disabled={busy || !folder}>{busy ? "Scanning…" : "Find duplicates"}</button>
      </PageHeader>
      <FolderField label="Look in" value={folder} onChange={setFolder} />
      <p className="muted">
        Files count as duplicates when their size and SHA-256 hash match. Nothing is ever deleted: moving extras aside
        keeps the oldest copy and puts the others in a Duplicates folder, where you can review them.
      </p>
      {groups && groups.length === 0 && (
        <EmptyState icon={<CheckCircleIcon size={30} />} title="No duplicates found">
          Every file in this folder is unique.
        </EmptyState>
      )}
      {groups && groups.length > 0 && (
        <div className="row">
          <span className="grow">
            {groups.length} group{groups.length === 1 ? "" : "s"}, {humanSize(wasted)} in extra copies.
          </span>
          <button onClick={() => moveAside(groups)}>Move all extra copies aside</button>
        </div>
      )}
      {groups?.map((g) => (
        <section className="card" key={g.sha256}>
          <div className="card-head">
            <h2>{fileName(g.files[0]?.path ?? "")}</h2>
            <span className="muted small">{humanSize(g.size_bytes)} · {g.files.length} copies · sha256 {g.sha256.slice(0, 16)}…</span>
            <button className="link" onClick={() => moveAside([g])}>Move extras aside</button>
          </div>
          {g.files.map((f) => (
            <div className="list-row" key={f.path}>
              <span className="ellipsis" title={f.path}>{f.path}</span>
              <span className="muted small">{dateTime(f.modified)}</span>
              <RevealButton path={f.path} />
            </div>
          ))}
        </section>
      ))}
    </div>
  );
}

const TOKENS = ["{date}", "{time}", "{original_name}", "{original_stem}", "{ext}", "{counter}"];

export function Rename() {
  const [folder, setFolder] = useStartFolder();
  const [template, setTemplate] = useState("{date}_{original_name}");
  const [rows, setRows] = useState<RenameRow[] | null>(null);
  const [busy, setBusy] = useState(false);

  async function preview() {
    setBusy(true);
    try {
      setRows(await api.renamePreview(folder, template));
    } catch (e) {
      setRows(null);
      toastError(e);
    } finally {
      setBusy(false);
    }
  }

  async function apply() {
    const n = rows?.filter((r) => !r.skipped && r.src !== r.dst).length ?? 0;
    if (!(await ask(`Rename ${n} file(s) in ${folder}?

You can undo this from History.`, { title: "Rename files", kind: "warning" }))) return;
    setBusy(true);
    try {
      const r = await api.renameApply(folder, template);
      toast(`${r.moved} renamed${r.failed.length ? `, ${r.failed.length} failed` : ""}`, r.failed.length > 0);
      setRows(null);
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  }

  const changes = rows?.filter((r) => !r.skipped && r.src !== r.dst).length ?? 0;

  return (
    <div className="page">
      <PageHeader title="Rename" />
      <FolderField label="Files in" value={folder} onChange={(p) => { setFolder(p); setRows(null); }} />
      <div className="row">
        <input className="grow mono" value={template} onChange={(e) => { setTemplate(e.target.value); setRows(null); }} />
        <button onClick={preview} disabled={busy || !folder}>Preview</button>
        <button className="primary" onClick={apply} disabled={busy || changes === 0}>Rename {changes || ""}</button>
      </div>
      <div className="chips">
        {TOKENS.map((t) => (
          <button key={t} className="chip mono" onClick={() => { setTemplate(template + t); setRows(null); }}>{t}</button>
        ))}
      </div>
      <p className="muted small">Only the files directly in this folder are renamed. Check the preview first; History can undo it.</p>
      {rows && (
        <table className="table">
          <thead><tr><th>Now</th><th>Becomes</th></tr></thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.src} className={r.skipped ? "dim" : ""}>
                <td>{fileName(r.src)}</td>
                <td>{r.skipped ? <span className="muted">{r.reason}</span> : fileName(r.dst)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
