import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useCallback, useEffect, useState } from "react";
import { PageHeader, pickFolder } from "../components/Common";
import { AlertIcon, CheckCircleIcon, ClockIcon, DropIcon, PauseIcon, PlayIcon } from "../components/Icons";
import { api } from "../lib/api";
import { fileName, humanSize } from "../lib/format";
import { openPreview, refreshConfig, refreshStatus, toast, toastError, useConfig, useStatus } from "../lib/store";
import type { Attention, FolderUsage } from "../lib/types";

export function Home() {
  const cfg = useConfig();
  const status = useStatus();
  const [usage, setUsage] = useState<FolderUsage[] | null>(null);
  const [dragging, setDragging] = useState(false);
  const [attention, setAttention] = useState<Attention[]>([]);

  const loadAttention = useCallback(() => {
    api.getAttention().then(setAttention).catch(toastError);
  }, []);
  useEffect(loadAttention, [loadAttention, status?.needs_attention]);

  const showPending = useCallback(async () => {
    try {
      openPreview(await api.pendingPlan());
    } catch (e) {
      toastError(e);
    }
  }, []);

  // Storage overview, refreshed whenever something was organized.
  useEffect(() => {
    api.storageOverview().then(setUsage).catch(toastError);
  }, [status?.organized_today, cfg?.settings.super_folder]);

  // Files dropped onto the window: plan them into the Super Folder.
  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent(async (e) => {
      const p = e.payload;
      if (p.type === "enter" || p.type === "over") setDragging(true);
      else if (p.type === "leave") setDragging(false);
      else if (p.type === "drop") {
        setDragging(false);
        try {
          const planned = await api.planPaths(p.paths);
          if (cfg?.settings.auto_organize) {
            const r = await api.executePlan(planned);
            toast(`${r.moved} file(s) organized${r.failed.length ? `, ${r.failed.length} failed` : ""}`, r.failed.length > 0);
          } else {
            openPreview(planned);
          }
        } catch (err) {
          toastError(err);
        }
      }
    });
    return () => void un.then((f) => f());
  }, [cfg?.settings.auto_organize]);

  if (!cfg) return null;
  const sf = cfg.settings.super_folder;

  if (!sf) {
    return <FirstRun suggested={cfg.suggested_super_folder} />;
  }

  return (
    <div className="page">
      <PageHeader title="Super Folder">
        <button onClick={() => api.openFolder(sf).catch(toastError)}>Open folder</button>
      </PageHeader>
      <p className="muted path-line" title={sf}>{sf}</p>

      <div className={dragging ? "dropzone active" : "dropzone"}>
        <div className="dropzone-icon"><DropIcon size={30} /></div>
        <div className="dropzone-title">Drop files here</div>
        <div className="muted small">or put them in the Super Folder: you'll see a preview before anything moves</div>
      </div>

      <div className="stats">
        <div className="stat accent">
          <span className="stat-icon"><ClockIcon size={20} /></span>
          <span className="stat-num">{status?.pending ?? 0}</span>
          <span className="stat-label">Files waiting</span>
        </div>
        <div className="stat ok">
          <span className="stat-icon"><CheckCircleIcon size={20} /></span>
          <span className="stat-num">{status?.organized_today ?? 0}</span>
          <span className="stat-label">Organized today</span>
        </div>
        <div className={status && status.needs_attention > 0 ? "stat warn" : "stat"}>
          <span className="stat-icon"><AlertIcon size={20} /></span>
          <span className="stat-num">{status?.needs_attention ?? 0}</span>
          <span className="stat-label">Need attention</span>
        </div>
      </div>

      <div className="row">
        <button className="primary big" onClick={showPending} disabled={!status || status.pending === 0}>
          Organize Files
        </button>
        <button className="with-icon" onClick={() => api.setPaused(!status?.paused).then(refreshStatus).catch(toastError)}>
          {status?.paused ? <PlayIcon size={15} /> : <PauseIcon size={15} />}
          {status?.paused ? "Resume monitoring" : "Pause monitoring"}
        </button>
      </div>
      {status?.watcher_error && <p className="error-text">Watcher problem: {status.watcher_error}</p>}

      {attention.length > 0 && (
        <section className="card">
          <div className="card-head">
            <h2>Needs attention</h2>
            <button className="link" onClick={() => api.clearAttention().then(loadAttention).catch(toastError)}>
              Clear list
            </button>
          </div>
          {attention.slice(-50).reverse().map((a) => (
            <div className="list-row" key={a.path}>
              <span title={a.path}>{fileName(a.path)}</span>
              <span className="muted">{a.reason}</span>
            </div>
          ))}
        </section>
      )}

      {usage && usage.length > 0 && <StorageCard usage={usage} />}
    </div>
  );
}

function StorageCard({ usage }: { usage: FolderUsage[] }) {
  const total = usage.reduce((n, u) => n + u.bytes, 0) || 1;
  const top = usage.slice(0, 8);
  return (
    <section className="card">
      <div className="card-head">
        <h2>Storage</h2>
        <span className="muted small">{humanSize(total)} in the Super Folder</span>
      </div>
      {top.map((u) => (
        <div className="usage-row" key={u.path}>
          <button className="link usage-name" title={u.path} onClick={() => api.openFolder(u.path).catch(toastError)}>
            {u.name}
          </button>
          <div className="usage-bar">
            <span style={{ width: `${Math.max(2, (u.bytes / total) * 100)}%` }} />
          </div>
          <span className="usage-size">{humanSize(u.bytes)}</span>
          <span className="usage-files muted small">{u.files} file{u.files === 1 ? "" : "s"}</span>
        </div>
      ))}
    </section>
  );
}

function FirstRun({ suggested }: { suggested: string | null }) {
  async function use(path: string) {
    try {
      const cfg = await api.getConfig();
      await api.saveSettings({ ...cfg.settings, super_folder: path });
      await refreshConfig();
      await refreshStatus();
      toast("Super Folder ready");
    } catch (e) {
      toastError(e);
    }
  }
  return (
    <div className="page">
      <PageHeader title="Welcome" />
      <section className="card first-run">
        <h2>Where should your Super Folder live?</h2>
        <p>Anything you put in it gets sorted into Images, Documents, Videos and so on, using simple rules you control. Nothing leaves this computer.</p>
        <div className="row">
          {suggested && (
            <button className="primary" onClick={() => use(suggested)}>
              Use {suggested}
            </button>
          )}
          <button
            onClick={async () => {
              const p = await pickFolder("Choose a Super Folder");
              if (p) await use(p);
            }}
          >
            Choose another folder…
          </button>
        </div>
      </section>
    </div>
  );
}
