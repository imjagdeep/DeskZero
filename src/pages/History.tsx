import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { EmptyState, PageHeader } from "../components/Common";
import { HistoryIcon } from "../components/Icons";
import { api } from "../lib/api";
import { dayLabel, fileName, parentDir, relTo } from "../lib/format";
import { toast, toastError, useConfig } from "../lib/store";
import type { HistoryEntry } from "../lib/types";
import { describe } from "../lib/undo";

/** Everything DeskZero did, organizer batches and file-browser actions,
 *  newest first. Any of it can be undone (Ctrl/Cmd+Z undoes the newest). */
export function History() {
  const cfg = useConfig();
  const root = cfg?.settings.organize_root ?? null;
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [os, setOs] = useState("");

  const load = useCallback(() => {
    api.getHistory().then(setEntries).catch(toastError);
  }, []);
  useEffect(() => {
    load();
    api.platform().then(setOs).catch(() => undefined);
    const un = listen("history-changed", load);
    return () => void un.then((f) => f());
  }, [load]);

  async function undo(e: HistoryEntry) {
    setBusy(e.id);
    try {
      const r = await api.undoBatch(e.id);
      toast(`Undid: ${describe(e)}${r.failed.length ? `, ${r.failed.length} couldn't be` : ""}`, r.failed.length > 0);
      load();
    } catch (err) {
      toastError(err);
    } finally {
      setBusy(null);
    }
  }

  const shown = entries.filter((e) => e.kind !== "undo");
  let lastDay = "";
  return (
    <div className="page">
      <PageHeader title="History" />
      <p className="muted">
        Everything DeskZero did: organized files, and renames, moves, copies, new folders and deletes in the file browser.
        {` ${os === "macos" ? "⌘Z" : "Ctrl+Z"} undoes the newest.`}
      </p>
      {shown.length === 0 && (
        <EmptyState icon={<HistoryIcon size={30} />} title="Nothing here yet">
          Every change DeskZero makes shows up here, and any of them can be undone.
        </EmptyState>
      )}
      {shown.map((e) => {
        const day = dayLabel(e.ts);
        const header = day !== lastDay ? <h3 className="day">{day}</h3> : null;
        lastDay = day;
        const time = new Date(e.ts).toLocaleTimeString(undefined, { timeStyle: "short" });
        // macOS lets only Finder put things back from the Trash.
        const macTrash = e.kind === "trashed" && os === "macos";
        return (
          <div key={e.id}>
            {header}
            <section className={e.undone ? "card dim" : "card"}>
              <div className="card-head">
                <span>
                  <strong>{describe(e)}</strong>{" "}
                  <span className="muted small">{time}{e.failed.length ? ` · ${e.failed.length} failed` : ""}</span>
                </span>
                {e.items.length > 0 &&
                  (e.undone ? (
                    <span className="muted small">undone</span>
                  ) : macTrash ? (
                    <span className="muted small">Put back from the Trash in Finder</span>
                  ) : (
                    <button onClick={() => undo(e)} disabled={busy !== null}>{busy === e.id ? "Undoing…" : "Undo"}</button>
                  ))}
              </div>
              {e.items.slice(0, 20).map((it) => (
                <div className="list-row" key={it.src + it.dst}>
                  <span>{fileName(it.src)}</span>
                  <span className="muted small ellipsis" title={it.src === it.dst ? it.src : `${it.src} → ${it.dst}`}>
                    {it.src === it.dst
                      ? relTo(parentDir(it.src), root)
                      : e.label === "Renamed"
                        ? `→ ${fileName(it.dst)}`
                        : `${relTo(parentDir(it.src), root)} → ${relTo(parentDir(it.dst), root)}`}
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
