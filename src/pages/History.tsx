import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { PageHeader } from "../components/Common";
import { api } from "../lib/api";
import { dayLabel, fileName, parentDir, relTo } from "../lib/format";
import { toast, toastError, useConfig } from "../lib/store";
import type { HistoryEntry } from "../lib/types";

export function History() {
  const cfg = useConfig();
  const root = cfg?.settings.organize_root ?? null;
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [busy, setBusy] = useState<string | null>(null);

  const load = useCallback(() => {
    api.getHistory().then(setEntries).catch(toastError);
  }, []);
  useEffect(() => {
    load();
    const un = listen("history-changed", load);
    return () => void un.then((f) => f());
  }, [load]);

  async function undo(e: HistoryEntry) {
    setBusy(e.id);
    try {
      const r = await api.undoBatch(e.id);
      toast(`${r.moved} file(s) moved back${r.failed.length ? `, ${r.failed.length} could not be` : ""}`, r.failed.length > 0);
      load();
    } catch (err) {
      toastError(err);
    } finally {
      setBusy(null);
    }
  }

  let lastDay = "";
  return (
    <div className="page">
      <PageHeader title="History & Undo" />
      {entries.length === 0 && <div className="empty">Nothing organized yet.</div>}
      {entries.map((e) => {
        const day = dayLabel(e.ts);
        const header = day !== lastDay ? <h3 className="day">{day}</h3> : null;
        lastDay = day;
        const time = new Date(e.ts).toLocaleTimeString(undefined, { timeStyle: "short" });
        return (
          <div key={e.id}>
            {header}
            <section className={e.undone ? "card dim" : "card"}>
              <div className="card-head">
                <span>
                  <strong>{e.kind === "undo" ? "Undo" : "Moved"}</strong>{" "}
                  <span className="muted small">{time} · {e.items.length} file(s){e.failed.length ? ` · ${e.failed.length} failed` : ""}</span>
                </span>
                {e.kind === "move" && e.items.length > 0 && (
                  e.undone ? <span className="muted small">undone</span> : (
                    <button onClick={() => undo(e)} disabled={busy !== null}>{busy === e.id ? "Undoing…" : "Undo"}</button>
                  )
                )}
              </div>
              {e.items.slice(0, 20).map((it) => (
                <div className="list-row" key={it.src + it.dst}>
                  <span>{fileName(it.src)}</span>
                  <span className="muted small ellipsis" title={`${it.src} → ${it.dst}`}>
                    {relTo(parentDir(it.src), root)} → {relTo(parentDir(it.dst), root)}
                  </span>
                </div>
              ))}
              {e.items.length > 20 && <div className="muted small">…and {e.items.length - 20} more</div>}
              {e.failed.map((f) => (
                <div className="list-row" key={f.src}>
                  <span className="error-text">{fileName(f.src)}</span>
                  <span className="muted small">{f.error}</span>
                </div>
              ))}
            </section>
          </div>
        );
      })}
    </div>
  );
}
