// Space-bar preview on Windows and Linux (macOS uses its own Quick Look):
// pictures and text files shown in place; anything else gets its details
// and an Open button. ← / → step through the files, Space or Esc closes.

import { useEffect, useState } from "react";
import { api, errorText } from "../lib/api";
import { dateTime, humanSize } from "../lib/format";
import type { BrowseItem } from "../lib/types";

const PICTURE = /\.(jpe?g|jfif|png|gif|webp|bmp|ico|tiff?|svg)$/i;

type Shown = { kind: "loading" } | { kind: "image"; url: string } | { kind: "text"; text: string } | { kind: "none"; note?: string };

export function PreviewSheet({
  files,
  path,
  onPath,
  onClose,
  onOpen,
}: {
  files: BrowseItem[];
  path: string;
  onPath: (p: string) => void;
  onClose: () => void;
  onOpen: (p: string) => void;
}) {
  const i = files.findIndex((f) => f.path === path);
  const item = files[i];
  const [shown, setShown] = useState<Shown>({ kind: "loading" });

  useEffect(() => {
    let live = true;
    setShown({ kind: "loading" });
    const load = PICTURE.test(path)
      ? api.previewImage(path).then((url): Shown => (url ? { kind: "image", url } : { kind: "none" }))
      : api.previewText(path).then((text): Shown => (text != null ? { kind: "text", text } : { kind: "none" }));
    load
      .then((s) => live && setShown(s))
      .catch((e) => live && setShown({ kind: "none", note: errorText(e) }));
    return () => {
      live = false;
    };
  }, [path]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" || e.key === " ") onClose();
      else if (e.key === "Enter") onOpen(path);
      else if ((e.key === "ArrowRight" || e.key === "ArrowDown") && i < files.length - 1) onPath(files[i + 1].path);
      else if ((e.key === "ArrowLeft" || e.key === "ArrowUp") && i > 0) onPath(files[i - 1].path);
      else return;
      e.preventDefault();
      e.stopPropagation();
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [i, files, path, onClose, onOpen, onPath]);

  if (!item) return null;
  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={`Preview of ${item.name}`} onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal preview-card">
        <div className="preview-head">
          <div className="preview-title" title={item.path}>{item.name}</div>
          <span className="muted small">
            {humanSize(item.size_bytes)} · {dateTime(item.modified)}
            {files.length > 1 && ` · ${i + 1} of ${files.length}`}
          </span>
        </div>
        <div className="preview-body">
          {shown.kind === "loading" && <div className="muted">Loading…</div>}
          {shown.kind === "image" && <img src={shown.url} alt="" draggable={false} />}
          {shown.kind === "text" && <pre>{shown.text}</pre>}
          {shown.kind === "none" && <div className="muted">{shown.note ?? "No preview for this kind of file."}</div>}
        </div>
        <div className="modal-actions">
          <button onClick={onClose}>Close</button>
          <button className="primary" onClick={() => onOpen(item.path)}>Open</button>
        </div>
      </div>
    </div>
  );
}
