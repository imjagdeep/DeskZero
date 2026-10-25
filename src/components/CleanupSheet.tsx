// "Clean up this folder": unfinished downloads, old installers, duplicate
// copies and very large files in the open folder. Everything ticked goes to
// the Trash / Recycle Bin (restorable, and Ctrl/Cmd+Z undoes it). Very large
// files are listed but not ticked: big doesn't mean unwanted.

import { useEffect, useMemo, useState } from "react";
import { api, errorText } from "../lib/api";
import { fileName, humanSize } from "../lib/format";
import type { CleanGroup } from "../lib/types";

export function CleanupSheet({
  dir,
  binName,
  onClose,
  onTrash,
}: {
  dir: string;
  binName: string;
  onClose: () => void;
  onTrash: (paths: string[]) => void;
}) {
  const [groups, setGroups] = useState<CleanGroup[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [ticked, setTicked] = useState<Set<string>>(new Set());

  useEffect(() => {
    api
      .cleanupSuggestions(dir)
      .then((g) => {
        setGroups(g);
        setTicked(new Set(g.filter((x) => x.id !== "large").flatMap((x) => x.items.map((i) => i.path))));
      })
      .catch((e) => setError(errorText(e)));
  }, [dir]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => e.key === "Escape" && (e.preventDefault(), onClose());
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onClose]);

  const sizes = useMemo(() => new Map((groups ?? []).flatMap((g) => g.items.map((i) => [i.path, i.size_bytes] as const))), [groups]);
  const bytes = [...ticked].reduce((n, p) => n + (sizes.get(p) ?? 0), 0);
  const toggle = (paths: string[], on: boolean) =>
    setTicked((t) => {
      const next = new Set(t);
      for (const p of paths) {
        if (on) next.add(p);
        else next.delete(p);
      }
      return next;
    });

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label="Clean up this folder" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal cleanup-card">
        <h2>Clean up {fileName(dir.replace(/[\\/]+$/, "")) || dir}</h2>
        {error && <p className="error-text">{error}</p>}
        {!groups && !error && <p className="muted">Looking through this folder…</p>}
        {groups && groups.length === 0 && <p className="muted">Nothing to clean up here. This folder looks tidy.</p>}
        {groups?.map((g) => {
          const paths = g.items.map((i) => i.path);
          const all = paths.every((p) => ticked.has(p));
          return (
            <section key={g.id} className="cleanup-group">
              <label className="cleanup-group-head">
                <input type="checkbox" checked={all} onChange={(e) => toggle(paths, e.target.checked)} />
                <span>{g.title}</span>
                <span className="muted small">{humanSize(g.items.reduce((n, i) => n + i.size_bytes, 0))}</span>
              </label>
              {g.items.map((it) => (
                <label key={it.path} className="cleanup-row" title={it.path}>
                  <input type="checkbox" checked={ticked.has(it.path)} onChange={(e) => toggle([it.path], e.target.checked)} />
                  <span className="cleanup-name">{fileName(it.path)}</span>
                  <span className="muted small">{it.reason}</span>
                  <span className="muted small cleanup-size">{humanSize(it.size_bytes)}</span>
                </label>
              ))}
            </section>
          );
        })}
        <div className="modal-actions">
          <button onClick={onClose}>Cancel</button>
          <button className="primary" disabled={ticked.size === 0} onClick={() => onTrash([...ticked])}>
            {ticked.size ? `Move ${ticked.size} to the ${binName} (${humanSize(bytes)})` : `Move to the ${binName}`}
          </button>
        </div>
      </div>
    </div>
  );
}
