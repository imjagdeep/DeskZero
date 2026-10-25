import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { FolderField, PageHeader, pickFolder } from "../components/Common";
import { DeskLabTools } from "../components/DeskLab";
import { ShortcutField } from "../components/ShortcutField";
import { api } from "../lib/api";
import { refreshConfig, refreshStatus, toast, toastError, useConfig } from "../lib/store";
import type { ConflictPolicy, FinderInfo, Settings as S, Theme } from "../lib/types";
import { getVersion } from "@tauri-apps/api/app";
import { checkAndDownload, installNow, useUpdate } from "../lib/updates";

export function Settings() {
  const cfg = useConfig();
  const [os, setOs] = useState("");
  const [tab, setTab] = useState<Tab>("files");
  useEffect(() => {
    api.platform().then(setOs).catch(toastError);
  }, []);
  if (!cfg) return null;
  const s = cfg.settings;
  const mac = os === "macos";

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

  const bin = os === "windows" ? "Recycle Bin" : "Trash";

  return (
    <div className="page settings-page">
      <PageHeader title="Settings" />
      <div className="settings-layout">
        {/* A section list on the left, like System Settings, Windows Settings
            and GNOME Settings. */}
        <nav className="settings-nav" role="tablist" aria-label="Settings sections" aria-orientation="vertical">
          {TABS.map((t) => (
            <button key={t.id} role="tab" aria-selected={tab === t.id} className={tab === t.id ? "settings-nav-item active" : "settings-nav-item"} onClick={() => setTab(t.id)}>
              {t.label}
            </button>
          ))}
        </nav>
        <div className="settings-body">

          {tab === "files" && (
            <>
              <section className="card">
                <h2>Browsing</h2>
                <label className="field">
                  <span>Open DeskZero on</span>
                  <select
                    value={s.start_folder ? "custom" : "downloads"}
                    onChange={async (e) => {
                      if (e.target.value === "downloads") return void save({ start_folder: null });
                      const p = await pickFolder("Folder DeskZero opens on");
                      if (p) await save({ start_folder: p });
                    }}
                  >
                    <option value="downloads">Downloads</option>
                    <option value="custom">{s.start_folder ?? "A folder I choose…"}</option>
                  </select>
                </label>
                <label className="check">
                  <input type="checkbox" checked={s.show_hidden} onChange={(e) => void save({ show_hidden: e.target.checked })} />
                  <span>Show hidden files<span className="muted small"> Also list dotfiles and files the system hides.</span></span>
                </label>
                <label className="check">
                  <input type="checkbox" checked={s.thumbnails} onChange={(e) => void save({ thumbnails: e.target.checked })} />
                  <span>Picture previews<span className="muted small"> Show photos and images as small pictures instead of icons. Made on this computer.</span></span>
                </label>
                <label className="check">
                  <input type="checkbox" checked={s.folder_sizes} onChange={(e) => void save({ folder_sizes: e.target.checked })} />
                  <span>Folder sizes<span className="muted small"> Add up each folder's size for its tile. Turn off if very large folders feel slow.</span></span>
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
            </>
          )}

          {tab === "editing" && (
            <section className="card">
              <h2>Editing files</h2>
              <label className="check">
                <input type="checkbox" checked={s.confirm_delete} onChange={(e) => void save({ confirm_delete: e.target.checked })} />
                <span>Ask before moving to the {bin}<span className="muted small"> Items you delete can always be put back from the {bin}.</span></span>
              </label>
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
                <span>Keep undo history for (days)</span>
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
              <p className="muted small">Copy and paste keep both files when names clash. Anything DeskZero changes can be undone from History.</p>
            </section>
          )}

          {tab === "features" && (
            <>
              <section className="card">
                <h2>Organizer</h2>
                <FolderField label="DeskZero folder" value={s.organize_root ?? ""} onChange={(p) => void save({ organize_root: p }).then((ok) => ok && toast("DeskZero changed; files already sorted stay where they are"))} />
                <label className="check">
                  <input type="checkbox" checked={s.auto_organize} onChange={(e) => void save({ auto_organize: e.target.checked })} />
                  <span>Automatic organization<span className="muted small"> Sort files dropped into your DeskZero right away, without a preview.</span></span>
                </label>
                <label className="check">
                  <input type="checkbox" checked={s.confirm_before_move} onChange={(e) => void save({ confirm_before_move: e.target.checked })} />
                  <span>Confirm before organizing watch folders<span className="muted small"> Off = new files in watch folders are moved as soon as they finish downloading.</span></span>
                </label>
                <label className="check">
                  <input type="checkbox" checked={s.watch_type_fallback} onChange={(e) => void save({ watch_type_fallback: e.target.checked })} />
                  <span>Sort watch-folder files by type when no rule matches<span className="muted small"> Moves them to your DeskZero by file type (png → Images, exe → Installers, zip → Archives). Off = they stay where they are.</span></span>
                </label>
              </section>

              <SearchSection settings={s} save={save} />

              <section className="card">
                <h2>Shortcuts</h2>
                <div className="shortcut-list">
                  <ShortcutField
                    label="Universal search"
                    value={s.search_shortcut}
                    fallback="CommandOrControl+Shift+Space"
                    mac={mac}
                    onSave={(v) => save({ search_shortcut: v }).then((ok) => (ok && toast("Shortcut saved"), ok))}
                  />
                  <ShortcutField
                    label="Organize waiting files"
                    value={s.organize_shortcut}
                    fallback="CommandOrControl+Alt+KeyO"
                    mac={mac}
                    onSave={(v) => save({ organize_shortcut: v }).then((ok) => (ok && toast("Shortcut saved"), ok))}
                  />
                  <div className="muted small">Works anywhere, even when DeskZero is in the {mac ? "menu bar" : "tray"}. Keys the system or another app already uses can't be taken.</div>
                </div>
                {os === "windows" && (
                  <label className="check">
                    <input type="checkbox" checked={s.context_menu} onChange={(e) => void save({ context_menu: e.target.checked })} />
                    <span>
                      Right-click menu in Explorer
                      <span className="muted small">
                        {" "}
                        Adds "Organize with DeskZero" when you right-click files or folders in Explorer (on Windows 11 it's under
                        "Show more options").
                      </span>
                    </span>
                  </label>
                )}
              </section>
            </>
          )}

          {tab === "advanced" && (
            <section className="card">
              <h2>Startup and notifications</h2>
              <label className="check">
                <input type="checkbox" checked={s.start_with_system} onChange={(e) => void save({ start_with_system: e.target.checked })} />
                <span>Start with system<span className="muted small"> Launch when you sign in, so folders stay organized.</span></span>
              </label>
              <label className="check">
                <input type="checkbox" checked={s.start_minimized} onChange={(e) => void save({ start_minimized: e.target.checked })} />
                <span>Start minimized<span className="muted small"> Open quietly in the {mac ? "menu bar" : "tray"} instead of showing the window.</span></span>
              </label>
              <label className="check">
                <input type="checkbox" checked={s.notifications} onChange={(e) => void save({ notifications: e.target.checked })} />
                <span>Notifications<span className="muted small"> Tell me when files are organized or waiting.</span></span>
              </label>
              <p className="muted small">
                Settings, rules and history are stored in{" "}
                <button className="link" onClick={() => api.openFolder(cfg.data_dir).catch(toastError)}>{cfg.data_dir}</button>.
              </p>
            </section>
          )}

          {tab === "about" && <AboutSection settings={s} save={save} dataDir={cfg.data_dir} />}
        </div>
      </div>
    </div>
  );
}

type Tab = "files" | "editing" | "features" | "advanced" | "about";
const TABS: { id: Tab; label: string }[] = [
  { id: "files", label: "Files" },
  { id: "editing", label: "Editing" },
  { id: "features", label: "Features" },
  { id: "advanced", label: "Advanced" },
  { id: "about", label: "About" },
];

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

function AboutSection({ settings: s, save, dataDir }: SectionProps & { dataDir: string }) {
  const [version, setVersion] = useState("");
  const st = useUpdate();
  useEffect(() => {
    getVersion().then(setVersion).catch(toastError);
  }, []);
  const busy = st.kind === "checking" || st.kind === "downloading" || st.kind === "installing";

  return (
    <section className="card">
      <h2>About</h2>
      <div className="about-row">
        <div>
          <div className="about-title">DeskZero {version && <span className="muted">version {version}</span>}</div>
          <div className="muted small">
            {st.kind === "idle" && (s.auto_update ? "Updates download automatically." : "Automatic updates are off.")}
            {st.kind === "checking" && "Checking GitHub for a newer version…"}
            {st.kind === "latest" && "You have the latest version."}
            {st.kind === "downloading" && `Downloading ${st.version}${st.pct != null ? ` (${st.pct}%)` : ""}…`}
            {st.kind === "ready" && `Version ${st.version} is downloaded and ready.`}
            {st.kind === "installing" && `Installing ${st.version}… DeskZero restarts by itself.`}
            {st.kind === "error" && <span className="error-text">Couldn't update: {st.message}</span>}
          </div>
        </div>
        {st.kind === "ready" ? (
          <button className="primary" onClick={() => void installNow()}>Restart to update</button>
        ) : (
          <button onClick={() => void checkAndDownload(true)} disabled={busy}>
            Check for updates
          </button>
        )}
      </div>
      {st.kind === "ready" && st.notes && <pre className="release-notes">{st.notes}</pre>}
      <label className="check">
        <input type="checkbox" checked={s.auto_update} onChange={(e) => void save({ auto_update: e.target.checked })} />
        <span>
          Download updates automatically
          <span className="muted small"> Checks GitHub once a day, downloads new versions in the background (signature-checked) and asks you to restart. Nothing else is sent.</span>
        </span>
      </label>
      <p className="muted small">
        Works offline. No account, no cloud, no telemetry. Updates are downloaded from github.com/imjagdeep/deskzero and
        signature-checked before installing. Settings, rules and history are stored in{" "}
        <button className="link" onClick={() => api.openFolder(dataDir).catch(toastError)}>{dataDir}</button>.
      </p>
      <div className="more-tools">
        <DeskLabTools own="deskzero" />
        <div className="muted small made-by">
          Made by Jagdeep Sandhu ·{" "}
          <button className="link" onClick={() => api.openLink("developer").catch(toastError)}>github.com/imjagdeep</button>
        </div>
      </div>
    </section>
  );
}
