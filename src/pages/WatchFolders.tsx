import { useEffect, useState } from "react";
import { EmptyState, PageHeader, pickFolder } from "../components/Common";
import { ContextMenu, type MenuItem } from "../components/ContextMenu";
import { FolderIcon, MoreIcon, WatchIcon } from "../components/Icons";
import { api } from "../lib/api";
import { fileName, parentDir } from "../lib/format";
import { openPreview, refreshConfig, refreshStatus, toast, toastError, useConfig, useStatus } from "../lib/store";
import type { Settings } from "../lib/types";

/** Ask the app to show a folder in DeskZero's own file browser. */
export const BROWSE_EVENT = "dz-browse";

export function WatchFolders() {
  const cfg = useConfig();
  const status = useStatus();
  const [os, setOs] = useState("");
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  useEffect(() => {
    api.platform().then(setOs).catch(() => undefined);
  }, []);
  if (!cfg) return null;
  const s = cfg.settings;
  const reveal = os === "macos" ? "Open in Finder" : os === "windows" ? "Open in File Explorer" : "Open in Files";

  async function save(next: Settings) {
    try {
      await api.saveSettings(next);
      await refreshConfig();
      await refreshStatus();
    } catch (e) {
      toastError(e);
    }
  }

  async function add() {
    const p = await pickFolder("Watch a folder");
    if (!p) return;
    if (s.watch_folders.some((w) => w.path === p)) return toast("That folder is already watched");
    await save({ ...s, watch_folders: [...s.watch_folders, { path: p, enabled: true }] });
  }

  async function sortNow(path: string) {
    try {
      const plan = await api.planWatchFolder(path);
      if (plan.length === 0) toast("Nothing to sort: no loose files in this folder.");
      else openPreview(plan);
    } catch (e) {
      toastError(e);
    }
  }

  function more(path: string, x: number, y: number) {
    setMenu({
      x,
      y,
      items: [
        { label: "Show in DeskZero", run: () => window.dispatchEvent(new CustomEvent(BROWSE_EVENT, { detail: path })) },
        { label: reveal, run: () => api.openFolder(path).catch(toastError) },
        "-",
        {
          label: "Stop watching",
          danger: true,
          run: () => void save({ ...s, watch_folders: s.watch_folders.filter((w) => w.path !== path) }).then(() => toast(`Stopped watching ${fileName(path)}`)),
        },
      ],
    });
  }

  return (
    <div className="page">
      <PageHeader title="Watch folders">
        <button className="primary" onClick={add}>Add folder…</button>
      </PageHeader>
      <p className="muted">New files in these folders are sorted by your rules, even ones that arrive while DeskZero is closed.</p>

      {s.watch_folders.length === 0 && (
        <EmptyState icon={<WatchIcon size={30} />} title="No watch folders yet">
          Downloads and Desktop are good places to start.
        </EmptyState>
      )}

      {s.watch_folders.length > 0 && (
        <section className="watch-list">
          {s.watch_folders.map((w) => {
            const running = w.enabled && status?.watching.includes(w.path);
            const state = !w.enabled ? "Paused" : status?.paused ? "Paused for all folders" : running ? "Watching" : "Not running";
            return (
              <div className={w.enabled ? "watch-card" : "watch-card off"} key={w.path}>
                <span className="watch-icon"><FolderIcon size={20} /></span>
                <span className="watch-text">
                  <span className="watch-name">{fileName(w.path) || w.path}</span>
                  <span className="watch-path" title={w.path}>{parentDir(w.path)}</span>
                </span>
                <span className={running && !status?.paused ? "watch-state on" : "watch-state"}>
                  <span className="dot" />
                  {state}
                </span>
                <button
                  title="Preview the files already in this folder; nothing moves until you confirm"
                  onClick={() => void sortNow(w.path)}
                >
                  Sort now…
                </button>
                <input
                  type="checkbox"
                  checked={w.enabled}
                  aria-label={`Watch ${fileName(w.path)}`}
                  title={w.enabled ? "Watching: switch off to pause this folder" : "Paused: switch on to watch this folder"}
                  onChange={(e) => {
                    const wf = s.watch_folders.map((x) => (x.path === w.path ? { ...x, enabled: e.target.checked } : x));
                    void save({ ...s, watch_folders: wf });
                  }}
                />
                <button
                  className="watch-more"
                  aria-label={`More for ${fileName(w.path)}`}
                  title="More"
                  onClick={(e) => {
                    const r = e.currentTarget.getBoundingClientRect();
                    more(w.path, r.left, r.bottom + 4);
                  }}
                >
                  <MoreIcon size={18} />
                </button>
              </div>
            );
          })}
        </section>
      )}

      <section className="card watch-how">
        <h2>How watching works</h2>
        <div className="list-row">
          <span>Files no rule matches</span>
          <span className="muted small">{s.watch_type_fallback ? "Filed by type into your DeskZero (png → Images)" : "Stay where they are"}</span>
        </div>
        <div className="list-row">
          <span>When files are moved</span>
          <span className="muted small">{s.confirm_before_move ? "After you confirm on the Overview tab" : "Right away, automatically"}</span>
        </div>
        <p className="muted small">Change these in Settings › Features. Everything moved can be undone from History.</p>
      </section>

      {menu && <ContextMenu {...menu} onClose={() => setMenu(null)} />}
    </div>
  );
}
