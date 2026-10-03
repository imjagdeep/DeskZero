import { useState } from "react";
import { FolderField, PageHeader } from "../components/Common";
import { api } from "../lib/api";
import { openPreview, toastError, useConfig } from "../lib/store";

const AGES = [30, 90, 180, 365];

function join(dir: string, name: string): string {
  const sep = dir.includes("\\") ? "\\" : "/";
  return dir.replace(/[\\/]+$/, "") + sep + name;
}

/** Move files nobody touched for a while into an Archive folder. */
export function Cleanup() {
  const cfg = useConfig();
  const downloads = cfg?.settings.watch_folders.find((w) => /downloads$/i.test(w.path))?.path;
  const [folder, setFolder] = useState<string | null>(null);
  const [dest, setDest] = useState<string | null>(null);
  const [days, setDays] = useState(90);
  const [busy, setBusy] = useState(false);

  const src = folder ?? downloads ?? cfg?.settings.organize_root ?? "";
  const target = dest ?? (src ? join(src, "Archive") : "");

  async function preview() {
    setBusy(true);
    try {
      openPreview(await api.planOldFiles(src, days, target));
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="page">
      <PageHeader title="Cleanup" />
      <p className="muted">
        Move files you haven't changed in a while out of the way. Nothing is deleted, you see the list first, and History
        can undo it.
      </p>
      <section className="card">
        <FolderField label="Clean up" value={src} onChange={(p) => { setFolder(p); setDest(null); }} />
        <div className="folder-field">
          <span className="folder-label">Older than</span>
          <div className="segmented">
            {AGES.map((d) => (
              <button key={d} className={days === d ? "active" : ""} onClick={() => setDays(d)}>
                {d >= 365 ? "1 year" : `${d} days`}
              </button>
            ))}
          </div>
        </div>
        <FolderField label="Move to" value={target} onChange={setDest} />
        <div className="row">
          <button className="primary" onClick={preview} disabled={busy || !src}>
            {busy ? "Looking…" : "Find old files"}
          </button>
          <span className="muted small">Only files directly in the folder; hidden and system files are left alone.</span>
        </div>
      </section>
    </div>
  );
}
