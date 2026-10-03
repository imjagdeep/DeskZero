import { useMemo, useState } from "react";
import { api } from "../lib/api";
import { fileName, humanSize, relTo } from "../lib/format";
import { closePreview, refreshStatus, toast, toastError, useConfig, usePreview } from "../lib/store";
import type { Decision, PlannedOp } from "../lib/types";

const isMove = (op: PlannedOp) => typeof op.status === "object" && "move" in op.status;

function skipReason(op: PlannedOp): string | null {
  if (op.status === "noop") return "already in place";
  if (typeof op.status === "object" && "skip" in op.status) return op.status.skip.reason;
  return null;
}

/** "Organize N files?" with Cancel / Organize, per the spec. Rendered once
 *  by the app; any page opens it with openPreview(plan). */
export function PreviewModal() {
  const plan = usePreview();
  if (!plan) return null;
  return <Sheet plan={plan} />;
}

function Sheet({ plan }: { plan: PlannedOp[] }) {
  const cfg = useConfig();
  const root = cfg?.settings.organize_root ?? null;
  const [busy, setBusy] = useState(false);
  const moves = useMemo(() => plan.filter(isMove), [plan]);
  const asks = useMemo(() => plan.filter((op) => op.status === "needs_decision"), [plan]);
  const others = useMemo(() => plan.filter((op) => !isMove(op) && op.status !== "needs_decision"), [plan]);
  const [choices, setChoices] = useState<Record<string, Decision>>({});
  const choice = (src: string): Decision => choices[src] ?? "keep_both";
  const count = moves.length + asks.filter((a) => choice(a.src) !== "skip").length;

  async function organize() {
    setBusy(true);
    try {
      const final = asks.length
        ? await api.resolveDecisions(plan, asks.map((a) => [a.src, choice(a.src)] as [string, Decision]))
        : plan;
      const r = await api.executePlan(final);
      const parts = [`${r.moved} file${r.moved === 1 ? "" : "s"} organized`];
      if (r.failed.length) parts.push(`${r.failed.length} failed`);
      toast(parts.join(", "), r.failed.length > 0);
      for (const f of r.failed.slice(0, 3)) toast(`${fileName(f.path)}: ${f.error}`, true);
      void refreshStatus();
      closePreview();
    } catch (e) {
      toastError(e);
      setBusy(false);
    }
  }

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="modal">
        <h2>{count ? `Organize ${count} file${count === 1 ? "" : "s"}?` : "Nothing to organize"}</h2>
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
          {asks.length > 0 && <h3>Already exists ({asks.length})</h3>}
          {asks.map((op) => (
            <div className="preview-row ask-row" key={op.src}>
              <div className="ask-text">
                <div className="preview-src" title={op.src}>{fileName(op.src)}</div>
                <div className="preview-dst" title={op.dst}>→ {relTo(op.dst, root)}</div>
              </div>
              <div className="segmented small-seg">
                {(["keep_both", "replace", "skip"] as Decision[]).map((d) => (
                  <button
                    key={d}
                    className={choice(op.src) === d ? "active" : ""}
                    onClick={() => setChoices({ ...choices, [op.src]: d })}
                  >
                    {d === "keep_both" ? "Keep both" : d === "replace" ? "Replace" : "Skip"}
                  </button>
                ))}
              </div>
            </div>
          ))}
          {others.length > 0 && <h3>Not moved ({others.length})</h3>}
          {others.map((op) => (
            <div className="preview-row dim" key={op.src}>
              <div className="preview-src" title={op.src}>{fileName(op.src)}</div>
              <div className="preview-dst">{skipReason(op)}</div>
            </div>
          ))}
        </div>
        {asks.some((a) => choice(a.src) === "replace") && (
          <p className="muted small">Replaced files are set aside, not deleted: Undo brings them back.</p>
        )}
        <div className="modal-actions">
          <button onClick={closePreview} disabled={busy}>Cancel</button>
          <button className="primary" onClick={organize} disabled={busy || count === 0}>
            {busy ? "Organizing…" : "Organize"}
          </button>
        </div>
      </div>
    </div>
  );
}
