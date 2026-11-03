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
  }, [status?.organized_today, cfg?.settings.organize_root]);

  // Files dropped onto the window: plan them into your DeskZero.
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
  const sf = cfg.settings.organize_root;

  if (!sf) {
    return <FirstRun suggested={cfg.suggested_organize_root} />;
  }

  const pending = status?.pending ?? 0;
  const watching = status?.watching ?? [];
  const where = watching.length === 1 ? fileName(watching[0].replace(/[\\/]+$/, "")) : `${watching.length} folders`;
  const state = status?.paused ? "Paused" : watching.length ? `Watching ${where}` : "Not watching any folder";

  return (
    <div className="page home-page">
      <header className="home-head">
        <div>
          <p className="muted">{state}</p>
          <h1>Your files, in order.</h1>
        </div>
        <button onClick={() => api.openFolder(sf).catch(toastError)}>Open folder</button>
      </header>

      <div className="home-cards">
        <button className={dragging ? "home-drop active" : "home-drop"} title={sf} onClick={() => api.openFolder(sf).catch(toastError)}>
          <span className="home-drop-icon">
            <DropIcon size={22} />
          </span>
          <strong>Drop files here</strong>
          <span className="home-drop-hint">You'll see where each one goes before anything moves.</span>
        </button>

        <section className="home-waiting" aria-label="Files waiting">
          <span className="home-card-title">
            <ClockIcon size={15} /> Waiting
          </span>
          <span className="home-big">{pending}</span>
          <span className="home-card-text">{pending === 1 ? "file to organize" : "files to organize"}</span>
          <div className="home-waiting-actions">
            <button className="primary" onClick={showPending} disabled={!status || pending === 0}>
              Organize now
            </button>
            <button className="ghost with-icon" onClick={() => api.setPaused(!status?.paused).then(refreshStatus).catch(toastError)}>
              {status?.paused ? <PlayIcon size={14} /> : <PauseIcon size={14} />}
              {status?.paused ? "Resume" : "Pause"}
            </button>
          </div>
        </section>

        <section className="home-today" aria-label="Today">
          <span className="home-card-title">
            <CheckCircleIcon size={15} /> Today
          </span>
          <span className="home-big">{status?.organized_today ?? 0}</span>
          <span className="home-card-text">organized</span>
          <span className={status && status.needs_attention > 0 ? "home-attention warn" : "home-attention"}>
            <AlertIcon size={14} /> {status?.needs_attention ?? 0} need attention
          </span>
        </section>
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
        <span className="muted small">{humanSize(total)} in your DeskZero</span>
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
      await api.saveSettings({ ...cfg.settings, organize_root: path });
      await refreshConfig();
      await refreshStatus();
      toast("DeskZero ready");
    } catch (e) {
      toastError(e);
    }
  }
  return (
    <div className="page">
      <PageHeader title="Welcome" />
      <section className="card first-run">
        <h2>Where should your DeskZero live?</h2>
        <p>Anything you put in it gets sorted into Images, Documents, Videos and so on, using simple rules you control. Nothing leaves this computer.</p>
        <div className="row">
          {suggested && (
            <button className="primary" onClick={() => use(suggested)}>
              Use {suggested}
            </button>
          )}
          <button
            onClick={async () => {
              const p = await pickFolder("Choose a DeskZero");
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
