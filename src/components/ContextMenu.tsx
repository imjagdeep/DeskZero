// A right-click menu in the same style as the menu bar's menus. Closes on
// Esc, a click elsewhere, scrolling or the window losing focus.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { accelText } from "../shell/menu";

export type MenuItem =
  | { label: string; accel?: string; disabled?: boolean; danger?: boolean; run: () => void }
  | "-";

export function ContextMenu({ x, y, items, onClose }: { x: number; y: number; items: MenuItem[]; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });

  // Keep the menu inside the window.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      left: Math.max(4, Math.min(x, window.innerWidth - r.width - 4)),
      top: Math.max(4, Math.min(y, window.innerHeight - r.height - 4)),
    });
    el.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [x, y]);

  useEffect(() => {
    const away = (e: Event) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const rows = [...(ref.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
        const i = rows.indexOf(document.activeElement as HTMLButtonElement);
        rows[(i + (e.key === "ArrowDown" ? 1 : -1) + rows.length) % rows.length]?.focus();
      }
    };
    window.addEventListener("mousedown", away, true);
    window.addEventListener("keydown", key, true);
    window.addEventListener("blur", onClose);
    window.addEventListener("wheel", onClose, { passive: true });
    return () => {
      window.removeEventListener("mousedown", away, true);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("blur", onClose);
      window.removeEventListener("wheel", onClose);
    };
  }, [onClose]);

  return (
    <div ref={ref} className="menu-pop ctx-menu" role="menu" style={pos} onContextMenu={(e) => e.preventDefault()}>
      {items.map((it, i) =>
        it === "-" ? (
          <div key={`sep${i}`} className="menu-sep" />
        ) : (
          <button
            key={it.label}
            role="menuitem"
            className={it.danger ? "menu-row danger" : "menu-row"}
            disabled={it.disabled}
            onMouseEnter={(e) => e.currentTarget.focus()}
            onClick={() => {
              onClose();
              it.run();
            }}
          >
            <span className="menu-check" />
            <span className="menu-label">{it.label}</span>
            {it.accel ? <span className="menu-accel">{accelText(it.accel)}</span> : <span />}
          </button>
        ),
      )}
    </div>
  );
}
