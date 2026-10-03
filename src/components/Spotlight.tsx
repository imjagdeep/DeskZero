// The universal search bar (separate frameless window, Ctrl/Cmd+Shift+Space).
// Type to search files, folders and apps; Enter opens, Ctrl/Cmd+Enter shows
// in folder, Esc or clicking elsewhere hides it.

import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";
import logo from "../assets/logo.png";
import { api, errorText } from "../lib/api";
import { parentDir } from "../lib/format";
import type { SearchEntry } from "../lib/types";
import { AppGridIcon, FileIcon, FolderIcon } from "./Icons";

/** C:\Users\me\Documents → ~\Documents (and /Users/me, /home/me). */
function shortPath(p: string): string {
  return p.replace(/^[A-Za-z]:\\Users\\[^\\]+(?=\\|$)/, "~").replace(/^\/(Users|home)\/[^/]+(?=\/|$)/, "~");
}

const KIND_LABEL: Record<SearchEntry["kind"], string> = { app: "Application", folder: "Folder", file: "File" };

export function Spotlight() {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchEntry[]>([]);
  const [sel, setSel] = useState(0);
  const [error, setError] = useState("");
  const input = useRef<HTMLInputElement>(null);
  const win = getCurrentWindow();

  const hide = useCallback(() => {
    void win.hide();
  }, [win]);

  // Focus and select on every show; hide when focus leaves.
  useEffect(() => {
    const unShown = listen("spotlight-shown", () => {
      setError("");
      input.current?.focus();
      input.current?.select();
    });
    const unFocus = win.onFocusChanged(({ payload: focused }) => {
      if (!focused) void win.hide();
    });
    input.current?.focus();
    return () => {
      void unShown.then((f) => f());
      void unFocus.then((f) => f());
    };
  }, [win]);

  useEffect(() => {
    let live = true;
    if (!query.trim()) {
      setHits([]);
      return;
    }
    api
      .universalSearch(query)
      .then((h) => {
        if (live) {
          setHits(h);
          setSel(0);
        }
      })
      .catch((e) => live && setError(errorText(e)));
    return () => {
      live = false;
    };
  }, [query]);

  async function open(hit: SearchEntry | undefined, reveal: boolean) {
    if (!hit) return;
    try {
      if (reveal) await api.revealFile(hit.path);
      else await api.launch(hit.path);
      hide();
    } catch (e) {
      setError(errorText(e));
    }
  }

  function onKey(e: React.KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      if (query) setQuery("");
      else hide();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setSel((s) => Math.min(s + 1, Math.max(hits.length - 1, 0)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSel((s) => Math.max(s - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      void open(hits[sel], e.ctrlKey || e.metaKey);
    }
  }

  return (
    <div className="spotlight" onKeyDown={onKey}>
      <div className="spot-bar">
        <img src={logo} alt="" className="spot-logo" />
        <input
          ref={input}
          className="spot-input"
          value={query}
          placeholder="Search files, folders and apps…"
          spellCheck={false}
          autoComplete="off"
          onChange={(e) => setQuery(e.target.value)}
        />
        <kbd className="spot-kbd">esc</kbd>
      </div>
      {(hits.length > 0 || error || query.trim()) && (
        <div className="spot-results">
          {error && <div className="spot-empty error-text">{error}</div>}
          {!error && hits.length === 0 && <div className="spot-empty">No matches for “{query.trim()}”</div>}
          {hits.map((h, i) => (
            <div
              key={h.path}
              className={i === sel ? "spot-row selected" : "spot-row"}
              onMouseEnter={() => setSel(i)}
              onClick={(e) => void open(h, e.ctrlKey || e.metaKey)}
            >
              <span className={`spot-icon ${h.kind}`}>
                {h.kind === "app" ? <AppGridIcon size={18} /> : h.kind === "folder" ? <FolderIcon size={18} /> : <FileIcon size={18} />}
              </span>
              <span className="spot-text">
                <span className="spot-name">{h.name}</span>
                <span className="spot-path">{h.kind === "app" ? KIND_LABEL.app : shortPath(parentDir(h.path))}</span>
              </span>
              {i === sel && <span className="spot-hint">{h.kind === "app" ? "Open" : "Open  ·  Ctrl+Enter show"}</span>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
