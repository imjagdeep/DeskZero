import { EmptyState, PageHeader, pickFolder } from "../components/Common";
import { WatchIcon } from "../components/Icons";
import { api } from "../lib/api";
import { openPreview, refreshConfig, refreshStatus, toast, toastError, useConfig, useStatus } from "../lib/store";
import type { Settings } from "../lib/types";

export function WatchFolders() {
  const cfg = useConfig();
  const status = useStatus();
  if (!cfg) return null;
  const s = cfg.settings;

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
    if (s.watch_folders.some((w) => w.path === p)) return;
    await save({ ...s, watch_folders: [...s.watch_folders, { path: p, enabled: true }] });
  }

  return (
    <div className="page">
      <PageHeader title="Watch Folders">
        <button className="primary" onClick={add}>Add folder…</button>
      </PageHeader>
      <p className="muted">
        New files that appear in these folders are sorted by your rules, including files that arrive while DeskZero is closed.
        {s.watch_type_fallback
          ? " Files no rule matches are filed by type into your DeskZero (png → Images, exe → Installers)."
          : " Files no rule matches stay where they are."}
        {" Use “Sort what's here…” for files that were already there."}
        {s.confirm_before_move ? " You confirm each batch on the Home page." : " Moves happen automatically."}
      </p>
      {s.watch_folders.length === 0 && (
        <EmptyState icon={<WatchIcon size={30} />} title="No watch folders yet">
          Downloads and Desktop are good places to start.
        </EmptyState>
      )}
      {s.watch_folders.map((w, i) => (
        <div className="list-row card-row" key={w.path}>
          <label className="toggle">
            <input
              type="checkbox"
              checked={w.enabled}
              onChange={(e) => {
                const wf = s.watch_folders.map((x, j) => (j === i ? { ...x, enabled: e.target.checked } : x));
                void save({ ...s, watch_folders: wf });
              }}
            />
            <span title={w.path}>{w.path}</span>
          </label>
          <span className="muted small">
            {w.enabled && status?.watching.includes(w.path) ? "● watching" : w.enabled ? "not running" : "off"}
          </span>
          <button
            className="link"
            title="Preview every file already in this folder; nothing moves until you confirm"
            onClick={async () => {
              try {
                const plan = await api.planWatchFolder(w.path);
                if (plan.length === 0) toast("Nothing to sort here: no loose files.");
                else openPreview(plan);
              } catch (e) {
                toastError(e);
              }
            }}
          >
            Sort what's here…
          </button>
          <button className="link" onClick={() => api.openFolder(w.path).catch(toastError)}>Open</button>
          <button
            className="link danger"
            onClick={() => void save({ ...s, watch_folders: s.watch_folders.filter((_, j) => j !== i) })}
          >
            Remove
          </button>
        </div>
      ))}
    </div>
  );
}
