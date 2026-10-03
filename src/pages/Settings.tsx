import { FolderField, PageHeader } from "../components/Common";
import { api } from "../lib/api";
import { refreshConfig, refreshStatus, toast, toastError, useConfig } from "../lib/store";
import type { ConflictPolicy, Settings as S, Theme } from "../lib/types";

export function Settings() {
  const cfg = useConfig();
  if (!cfg) return null;
  const s = cfg.settings;

  async function save(patch: Partial<S>): Promise<boolean> {
    try {
      await api.saveSettings({ ...s, ...patch });
      await refreshConfig();
      await refreshStatus();
      return true;
    } catch (e) {
      toastError(e);
      return false;
    }
  }

  return (
    <div className="page">
      <PageHeader title="Settings" />

      <section className="card">
        <h2>General</h2>
        <label className="check">
          <input type="checkbox" checked={s.start_with_system} onChange={(e) => void save({ start_with_system: e.target.checked })} />
          <span>Start with system<span className="muted small"> Launch when you sign in, so folders stay organized.</span></span>
        </label>
        <label className="check">
          <input type="checkbox" checked={s.start_minimized} onChange={(e) => void save({ start_minimized: e.target.checked })} />
          <span>Start minimized<span className="muted small"> Open quietly in the tray instead of showing the window.</span></span>
        </label>
        <label className="check">
          <input type="checkbox" checked={s.notifications} onChange={(e) => void save({ notifications: e.target.checked })} />
          <span>Notifications<span className="muted small"> Tell me when files are organized or waiting.</span></span>
        </label>
      </section>

      <section className="card">
        <h2>Super Folder</h2>
        <FolderField label="Location" value={s.super_folder ?? ""} onChange={(p) => void save({ super_folder: p }).then((ok) => ok && toast("Super Folder changed; files already sorted stay where they are"))} />
        <label className="check">
          <input type="checkbox" checked={s.auto_organize} onChange={(e) => void save({ auto_organize: e.target.checked })} />
          <span>Automatic organization<span className="muted small"> Sort files dropped into the Super Folder right away, without a preview.</span></span>
        </label>
        <label className="check">
          <input type="checkbox" checked={s.confirm_before_move} onChange={(e) => void save({ confirm_before_move: e.target.checked })} />
          <span>Confirm before organizing watch folders<span className="muted small"> Off = new files in watch folders are moved as soon as they finish downloading.</span></span>
        </label>
      </section>

      <section className="card">
        <h2>Safety</h2>
        <label className="field">
          <span>When a file with the same name already exists</span>
          <select value={s.conflict_policy} onChange={(e) => void save({ conflict_policy: e.target.value as ConflictPolicy })}>
            <option value="rename">Keep both: name it "file (1)" (default)</option>
            <option value="skip">Skip: leave the new file where it is</option>
            <option value="replace">Replace: the old file is kept aside so Undo can restore it</option>
            <option value="ask">Ask me</option>
          </select>
        </label>
        <label className="field">
          <span>Keep history for (days)</span>
          <input
            type="number"
            min={1}
            max={3650}
            value={s.keep_history_days}
            onChange={(e) => {
              const n = Math.round(Number(e.target.value));
              if (n >= 1 && n <= 3650) void save({ keep_history_days: n });
            }}
          />
        </label>
      </section>

      <section className="card">
        <h2>Appearance</h2>
        <div className="segmented">
          {(["light", "dark", "system"] as Theme[]).map((t) => (
            <button key={t} className={s.theme === t ? "active" : ""} onClick={() => void save({ theme: t })}>
              {t[0].toUpperCase() + t.slice(1)}
            </button>
          ))}
        </div>
      </section>

      <section className="card">
        <h2>About</h2>
        <p className="muted small">
          Works fully offline. No account, no cloud, no telemetry. Settings, rules and history are stored in{" "}
          <button className="link" onClick={() => api.openFolder(cfg.data_dir).catch(toastError)}>{cfg.data_dir}</button>.
        </p>
      </section>
    </div>
  );
}
