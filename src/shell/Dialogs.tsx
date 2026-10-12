// About, Keyboard shortcuts and the command palette (Ctrl/Cmd+K).
// Same file in DeskZero, DeskMedic and DeskFolio.

import { useEffect, useMemo, useRef, useState } from "react";
import { accelText, allCommands, runCmd, type MenuDef } from "./menu";
import { closeShellDialog, useShellDialog } from "./shell";

export interface AboutInfo {
  name: string;
  logo: string;
  version: string;
  tagline: string;
  links: { label: string; open: () => void }[];
}

export function ShellDialogs({ menus, about, onError }: { menus: MenuDef[]; about: AboutInfo; onError: (e: unknown) => void }) {
  const which = useShellDialog();
  // Esc closes whichever is open.
  useEffect(() => {
    if (!which) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        closeShellDialog();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [which]);

  if (which === "about") return <About info={about} />;
  if (which === "shortcuts") return <Shortcuts menus={menus} />;
  if (which === "palette") return <Palette menus={menus} onError={onError} />;
  return null;
}

function Backdrop({ children, label }: { children: React.ReactNode; label: string }) {
  return (
    <div
      className="modal-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label={label}
      onMouseDown={(e) => e.target === e.currentTarget && closeShellDialog()}
    >
      {children}
    </div>
  );
}

function About({ info }: { info: AboutInfo }) {
  return (
    <Backdrop label={`About ${info.name}`}>
      <div className="modal about-card">
        <img src={info.logo} alt="" className="about-logo" />
        <h2>{info.name}</h2>
        <div className="muted">Version {info.version}</div>
        <p className="about-tagline">{info.tagline}</p>
        <div className="about-links">
          {info.links.map((l) => (
            <button key={l.label} className="link" onClick={l.open}>
              {l.label}
            </button>
          ))}
        </div>
        <p className="muted small">
          Free and open source (MIT licence). Works offline: no account, no ads, no telemetry.
          <br />© 2026 Jagdeep Sandhu · part of DeskLab
        </p>
        <div className="modal-actions">
          <button className="primary" autoFocus onClick={closeShellDialog}>
            OK
          </button>
        </div>
      </div>
    </Backdrop>
  );
}

function Shortcuts({ menus }: { menus: MenuDef[] }) {
  const groups = menus
    .map((m) => ({ label: m.label, cmds: allCommands([m]).filter(({ cmd }) => cmd.accel) }))
    .filter((g) => g.cmds.length > 0);
  return (
    <Backdrop label="Keyboard shortcuts">
      <div className="modal shortcuts-card">
        <h2>Keyboard shortcuts</h2>
        <div className="shortcut-cols">
          {groups.map((g) => (
            <section key={g.label}>
              <h3>{g.label}</h3>
              {g.cmds.map(({ cmd }) => (
                <div className="shortcut-row" key={cmd.id}>
                  <span>{cmd.label.replace(/…$/, "")}</span>
                  <kbd>{accelText(cmd.accel ?? "")}</kbd>
                </div>
              ))}
            </section>
          ))}
        </div>
        <div className="modal-actions">
          <button className="primary" onClick={closeShellDialog}>
            Done
          </button>
        </div>
      </div>
    </Backdrop>
  );
}

/** Loose match: every typed word appears somewhere in the label or its menu. */
function score(text: string, q: string): number {
  const t = text.toLowerCase();
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.every((w) => t.includes(w))) return -1;
  return t.startsWith(words[0] ?? "") ? 2 : 1;
}

function Palette({ menus, onError }: { menus: MenuDef[]; onError: (e: unknown) => void }) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const list = useRef<HTMLDivElement>(null);
  const all = useMemo(
    () => allCommands(menus).filter(({ cmd }) => !cmd.hidden && (!cmd.enabled || cmd.enabled())),
    [menus],
  );
  const hits = useMemo(() => {
    if (!q.trim()) return all;
    return all
      .map((h) => ({ h, s: Math.max(score(h.cmd.label, q) * 2, score(`${h.path} ${h.cmd.label}`, q)) }))
      .filter((x) => x.s > 0)
      .sort((a, b) => b.s - a.s)
      .map((x) => x.h);
  }, [all, q]);

  useEffect(() => setSel(0), [q]);
  useEffect(() => {
    list.current?.querySelector(".palette-row.selected")?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const go = (i: number) => {
    const h = hits[i];
    if (!h) return;
    closeShellDialog();
    void runCmd(h.cmd, onError);
  };

  return (
    <Backdrop label="Command search">
      <div className="palette">
        <input
          className="palette-input"
          autoFocus
          placeholder="Type a command, page or setting…"
          value={q}
          spellCheck={false}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") setSel((s) => Math.min(s + 1, hits.length - 1));
            else if (e.key === "ArrowUp") setSel((s) => Math.max(s - 1, 0));
            else if (e.key === "Enter") go(sel);
            else return;
            e.preventDefault();
          }}
        />
        <div className="palette-list" ref={list} role="listbox">
          {hits.length === 0 && <div className="palette-empty">Nothing matches “{q.trim()}”</div>}
          {hits.map((h, i) => (
            <div
              key={h.cmd.id}
              role="option"
              aria-selected={i === sel}
              className={i === sel ? "palette-row selected" : "palette-row"}
              onMouseEnter={() => setSel(i)}
              onMouseDown={(e) => {
                e.preventDefault();
                go(i);
              }}
            >
              <span className="palette-label">{h.cmd.label.replace(/…$/, "")}</span>
              <span className="palette-path">{h.path}</span>
              {h.cmd.accel && <kbd>{accelText(h.cmd.accel)}</kbd>}
            </div>
          ))}
        </div>
      </div>
    </Backdrop>
  );
}
