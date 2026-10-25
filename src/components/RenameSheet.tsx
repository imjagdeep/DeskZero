// Rename several selected files at once with a template, e.g.
// {date}_{original_name}. Shows every new name before anything changes;
// the batch lands in History, so Ctrl/Cmd+Z puts the old names back.

import { useEffect, useState } from "react";
import { api, errorText } from "../lib/api";
import { fileName } from "../lib/format";
import { toast, toastError } from "../lib/store";
import type { RenameRow } from "../lib/types";

const TOKENS: { token: string; help: string }[] = [
  { token: "{date}", help: "date taken or modified" },
  { token: "{time}", help: "time" },
  { token: "{original_name}", help: "name with extension" },
  { token: "{original_stem}", help: "name without extension" },
  { token: "{ext}", help: "extension" },
  { token: "{counter}", help: "1, 2, 3…" },
];

export function RenameSheet({ paths, onClose, onDone }: { paths: string[]; onClose: () => void; onDone: () => void }) {
  const [template, setTemplate] = useState("{date}_{original_name}");
  const [rows, setRows] = useState<RenameRow[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Live preview, a moment after typing stops.
  useEffect(() => {
    let live = true;
    const t = window.setTimeout(() => {
      api
        .renamePreview(paths, template)
        .then((r) => live && (setRows(r), setError(null)))
        .catch((e) => live && (setRows(null), setError(errorText(e))));
    }, 200);
    return () => {
      live = false;
      window.clearTimeout(t);
    };
  }, [paths, template]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => e.key === "Escape" && (e.preventDefault(), onClose());
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onClose]);

  const changes = rows?.filter((r) => !r.skipped && r.src !== r.dst).length ?? 0;

  async function apply() {
    setBusy(true);
    try {
      const r = await api.renameApply(paths, template);
      toast(`${r.moved} renamed${r.failed.length ? `, ${r.failed.length} failed` : ""}`, r.failed.length > 0);
      onDone();
    } catch (e) {
      toastError(e);
      setBusy(false);
    }
  }

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label="Rename files" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal rename-card">
        <h2>Rename {paths.length} files</h2>
        <input
          className="mono"
          value={template}
          onChange={(e) => setTemplate(e.target.value)}
          aria-label="New name pattern"
          spellCheck={false}
          autoFocus
        />
        <div className="chips">
          {TOKENS.map((t) => (
            <button key={t.token} className="chip mono" title={t.help} onClick={() => setTemplate(template + t.token)}>
              {t.token}
            </button>
          ))}
        </div>
        {error && <p className="error-text">{error}</p>}
        {rows && (
          <div className="rename-preview">
            {rows.map((r) => (
              <div key={r.src} className={r.skipped ? "rename-row dim" : "rename-row"}>
                <span className="ellipsis" title={r.src}>{fileName(r.src)}</span>
                <span className="muted">→</span>
                <span className="ellipsis">{r.skipped ? <span className="muted">{r.reason}</span> : fileName(r.dst)}</span>
              </div>
            ))}
          </div>
        )}
        <div className="modal-actions">
          <button onClick={onClose}>Cancel</button>
          <button className="primary" disabled={busy || changes === 0} onClick={() => void apply()}>
            Rename {changes || ""}
          </button>
        </div>
      </div>
    </div>
  );
}
