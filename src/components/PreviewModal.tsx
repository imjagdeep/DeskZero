import { useMemo, useState } from "react";
import { api } from "../lib/api";
import { fileName, humanSize, relTo } from "../lib/format";
import { refreshStatus, toast, toastError, useConfig } from "../lib/store";
import type { PlannedOp } from "../lib/types";

function skipReason(op: PlannedOp): string | null {
  if (op.status === "noop") return "already in place";
  if (op.status === "needs_decision") return "a file with this name exists: skipped for now";
  if ("skip" in op.status) return op.status.skip.reason;
  return null;
}

/** "Organize N files?" with Cancel / Organize, per the spec. */
export function PreviewModal({ plan, onClose }: { plan: PlannedOp[]; onClose: (didRun: boolean) => void }) {
  const cfg = useConfig();
  const root = cfg?.settings.super_folder ?? null;
  const [busy, setBusy] = useState(false);
  const moves = useMemo(() => plan.filter((op) => typeof op.status === "object" && "move" in op.status), [plan]);
  const others = useMemo(() => plan.filter((op) => !moves.includes(op)), [plan, moves]);

  async function organize() {
    setBusy(true);
    try {
      const r = await api.executePlan(plan);
      const parts = [`${r.moved} file(s) organized`];
      if (r.failed.length) parts.push(`${r.failed.length} failed`);
      toast(parts.join(", "), r.failed.length > 0);
      for (const f of r.failed.slice(0, 3)) toast(`${fileName(f.path)}: ${f.error}`, true);
      void refreshStatus();
      onClose(true);
    } catch (e) {
      toastError(e);
      setBusy(false);
    }
  }

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="modal">
        <h2>{moves.length ? `Organize ${moves.length} file${moves.length === 1 ? "" : "s"}?` : "Nothing to organize"}</h2>
        <div className="preview-list">
          {moves.map((op) => (
            <div className="preview-row" key={op.src}>
              <div className="preview-src" title={op.src}>
                {fileName(op.src)} <span className="muted">{humanSize(op.size_bytes)}</span>
              </div>
              <div className="preview-dst" title={op.dst}>
                → {relTo(op.dst, root)}
                {typeof op.status === "object" && "move" in op.status && op.status.move.replace && (
                  <span className="badge warn">replaces existing</span>
                )}
                {op.rule_name && <span className="badge">{op.rule_name}</span>}
              </div>
            </div>
          ))}
          {others.length > 0 && <h3 className="muted">Not moved ({others.length})</h3>}
          {others.map((op) => (
            <div className="preview-row dim" key={op.src}>
              <div className="preview-src" title={op.src}>{fileName(op.src)}</div>
              <div className="preview-dst muted">{skipReason(op)}</div>
            </div>
          ))}
        </div>
        <div className="modal-actions">
          <button onClick={() => onClose(false)} disabled={busy}>Cancel</button>
          <button className="primary" onClick={organize} disabled={busy || moves.length === 0}>
            {busy ? "Organizing…" : "Organize"}
          </button>
        </div>
      </div>
    </div>
  );
}
