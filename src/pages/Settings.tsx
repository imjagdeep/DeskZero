import { listen } from "@tauri-apps/api/event";
import type { Update } from "@tauri-apps/plugin-updater";
import { useEffect, useState } from "react";
import { FolderField, PageHeader, pickFolder } from "../components/Common";
import { api, errorText } from "../lib/api";
import { refreshConfig, refreshStatus, toast, toastError, useConfig } from "../lib/store";
import type { ConflictPolicy, FinderInfo, Settings as S, Theme } from "../lib/types";
import { appVersion, findUpdate, installUpdate } from "../lib/updates";

export function Settings() {
  const cfg = useConfig();
  const [os, setOs] = useState("");
  useEffect(() => {
    api.platform().then(setOs).catch(toastError);
  }, []);
  if (!cfg) return null;
  const s = cfg.settings;
  const mod = os === "macos" ? "⌘" : "Ctrl";

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
        {os === "windows" && (
          <label className="check">
            <input type="checkbox" checked={s.context_menu} onChange={(e) => void save({ context_menu: e.target.checked })} />
            <span>
              Right-click menu
              <span className="muted small">
                {" "}
                Adds "Organize with DeskZero" when you right-click files or folders in Explorer (on Windows 11 it's under
                "Show more options").
              </span>
            </span>
          </label>
        )}
        <div className="shortcut-list">
          <div><kbd>{mod}</kbd>+<kbd>Shift</kbd>+<kbd>Space</kbd><span className="muted">Universal search</span></div>
          <div><kbd>{mod}</kbd>+<kbd>{os === "macos" ? "⌥" : "Alt"}</kbd>+<kbd>O</kbd><span className="muted">Organize waiting files</span></div>
        </div>
      </section>

      <section className="card">
        <h2>DeskZero</h2>
        <FolderField label="Location" value={s.organize_root ?? ""} onChange={(p) => void save({ organize_root: p }).then((ok) => ok && toast("DeskZero changed; files already sorted stay where they are"))} />
        <label className="check">
          <input type="checkbox" checked={s.auto_organize} onChange={(e) => void save({ auto_organize: e.target.checked })} />
          <span>Automatic organization<span className="muted small"> Sort files dropped into your DeskZero right away, without a preview.</span></span>
        </label>
        <label className="check">
          <input type="checkbox" checked={s.confirm_before_move} onChange={(e) => void save({ confirm_before_move: e.target.checked })} />
          <span>Confirm before organizing watch folders<span className="muted small"> Off = new files in watch folders are moved as soon as they finish downloading.</span></span>
        </label>
      </section>

      <SearchSection settings={s} save={save} />

      <section className="card">
        <h2>Safety</h2>
        <label className="field">
          <span>When a file with the same name already exists</span>
          <select value={s.conflict_policy} onChange={(e) => void save({ conflict_policy: e.target.value as ConflictPolicy })}>
            <option value="rename">Keep both: name it "file (1)" (default)</option>
            <option value="skip">Skip: leave the new file where it is</option>
            <option value="replace">Replace: the old file is kept aside so Undo can restore it</option>
            <option value="ask">Ask me each time</option>
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

      <AboutSection settings={s} save={save} dataDir={cfg.data_dir} />
    </div>
  );
}

type SectionProps = { settings: S; save: (patch: Partial<S>) => Promise<boolean> };

function SearchSection({ settings: s, save }: SectionProps) {
  const [info, setInfo] = useState<FinderInfo | null>(null);
  useEffect(() => {
    const load = () => api.searchInfo().then(setInfo).catch(toastError);
    load();
    const un = listen("search-index-ready", load);
    return () => void un.then((f) => f());
  }, [s.search_roots]);

  const custom = s.search_roots.length > 0;
  return (
    <section className="card">
      <div className="card-head">
        <h2>Universal search</h2>
        <button
          className="link"
          disabled={info?.building}
          onClick={() => api.rebuildSearch().then(() => setInfo(info && { ...info, building: true })).catch(toastError)}
        >
          {info?.building ? "Updating…" : "Update now"}
        </button>
      </div>
      <p className="muted small">
        {info ? `${info.entries.toLocaleString()} files, folders and apps indexed on this computer. ` : ""}
        The list refreshes every 20 minutes and never leaves this computer.
      </p>
      {!custom && (
        <p className="small">Searching your usual folders (Desktop, Documents, Downloads, Pictures, Music, Videos) plus your DeskZero and watch folders.</p>
      )}
      {custom &&
        s.search_roots.map((r) => (
          <div className="list-row" key={r}>
            <span title={r}>{r}</span>
            <button className="link danger" onClick={() => void save({ search_roots: s.search_roots.filter((x) => x !== r) })}>
              Remove
            </button>
          </div>
        ))}
      <div className="row">
        <button
          onClick={async () => {
            const p = await pickFolder("Add a folder to search");
            if (p && !s.search_roots.includes(p)) await save({ search_roots: [...s.search_roots, p] });
          }}
        >
          {custom ? "Add folder…" : "Choose my own folders…"}
        </button>
        {custom && <button onClick={() => void save({ search_roots: [] })}>Use the usual folders</button>}
      </div>
    </section>
  );
}

type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "latest" }
  | { kind: "available"; update: Update }
  | { kind: "installing"; pct: number | null }
  | { kind: "error"; message: string };

function AboutSection({ settings: s, save, dataDir }: SectionProps & { dataDir: string }) {
  const [version, setVersion] = useState("");
  const [st, setSt] = useState<UpdateState>({ kind: "idle" });
  useEffect(() => {
    appVersion().then(setVersion).catch(toastError);
  }, []);

  async function check() {
    setSt({ kind: "checking" });
    try {
      const u = await findUpdate();
      setSt(u ? { kind: "available", update: u } : { kind: "latest" });
    } catch (e) {
      setSt({ kind: "error", message: errorText(e) });
    }
  }

  async function install(update: Update) {
    setSt({ kind: "installing", pct: 0 });
    try {
      await installUpdate(update, (pct) => setSt({ kind: "installing", pct }));
    } catch (e) {
      setSt({ kind: "error", message: errorText(e) });
    }
  }

  return (
    <section className="card">
      <h2>About</h2>
      <div className="about-row">
        <div>
          <div className="about-title">DeskZero {version && <span className="muted">version {version}</span>}</div>
          <div className="muted small">
            {st.kind === "idle" && "Updates are only checked when you ask."}
            {st.kind === "checking" && "Checking GitHub for a newer version…"}
            {st.kind === "latest" && "You have the latest version."}
            {st.kind === "available" && `Version ${st.update.version} is available.`}
            {st.kind === "installing" && `Downloading and installing${st.pct != null ? ` (${st.pct}%)` : ""}… the app restarts when done.`}
            {st.kind === "error" && <span className="error-text">Couldn't check for updates: {st.message}</span>}
          </div>
        </div>
        {st.kind === "available" ? (
          <button className="primary" onClick={() => void install(st.update)}>Download & install</button>
        ) : (
          <button onClick={() => void check()} disabled={st.kind === "checking" || st.kind === "installing"}>
            Check for updates
          </button>
        )}
      </div>
      {st.kind === "available" && st.update.body && <pre className="release-notes">{st.update.body}</pre>}
      <label className="check">
        <input type="checkbox" checked={s.check_updates_weekly} onChange={(e) => void save({ check_updates_weekly: e.target.checked })} />
        <span>Check for updates weekly<span className="muted small"> The only time DeskZero goes online. Off by default.</span></span>
      </label>
      <p className="muted small">
        Works offline. No account, no cloud, no telemetry. Updates are downloaded from github.com/imjagdeep/deskzero and
        signature-checked before installing. Settings, rules and history are stored in{" "}
        <button className="link" onClick={() => api.openFolder(dataDir).catch(toastError)}>{dataDir}</button>.
      </p>
    </section>
  );
}
