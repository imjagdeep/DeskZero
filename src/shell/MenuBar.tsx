// The in-window menu bar (File, Edit, View, Help) for Windows and Linux;
// on a Mac the same menus go into the system menu bar instead. Handles the
// keyboard shortcuts too. Same file in DeskZero, DeskMedic and DeskFolio.

import { useEffect, useRef, useState } from "react";
import { accelText, isCmd, isMac, isSub, matches, allCommands, runCmd, setNativeMenu, type Entry, type MenuDef } from "./menu";

export function MenuBar({ appName, menus, onError }: { appName: string; menus: MenuDef[]; onError: (e: unknown) => void }) {
  const [open, setOpen] = useState<number | null>(null);
  const [hot, setHot] = useState(-1);
  const bar = useRef<HTMLDivElement>(null);
  // Keep the latest menus for the key handler without re-binding it.
  const latest = useRef(menus);
  latest.current = menus;

  // macOS: the system menu bar. Rebuilt when the menus change (labels, checks).
  useEffect(() => {
    if (isMac) void setNativeMenu(appName, menus, onError).catch(onError);
  }, [appName, menus, onError]);

  // Keyboard shortcuts (on a Mac the system menu handles them itself).
  useEffect(() => {
    if (isMac) return;
    const onKey = (e: KeyboardEvent) => {
      for (const { cmd } of allCommands(latest.current)) {
        if (!cmd.accel || cmd.native || !matches(e, cmd.accel)) continue;
        // Plain typing keys stay with text fields.
        e.preventDefault();
        void runCmd(cmd, onError);
        return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onError]);

  // Close on a click elsewhere or when the window loses focus.
  useEffect(() => {
    if (open === null) return;
    const close = (e: MouseEvent) => {
      if (!bar.current?.contains(e.target as Node)) setOpen(null);
    };
    const blur = () => setOpen(null);
    window.addEventListener("mousedown", close);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("blur", blur);
    };
  }, [open]);

  // Alt on its own (Windows habit) opens the first menu; F10 too.
  useEffect(() => {
    if (isMac) return;
    let altAlone = false;
    const down = (e: KeyboardEvent) => {
      altAlone = e.key === "Alt" && !e.repeat;
      if (e.key === "F10" && !e.shiftKey) {
        e.preventDefault();
        setOpen((o) => (o === null ? 0 : null));
        setHot(-1);
      }
    };
    const up = (e: KeyboardEvent) => {
      if (e.key === "Alt" && altAlone) {
        e.preventDefault();
        setOpen((o) => (o === null ? 0 : null));
        setHot(-1);
      }
      altAlone = false;
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
    };
  }, []);

  // Arrow keys, Enter and Esc while a menu is open (focus stays where it was).
  useEffect(() => {
    if (open === null) return;
    const items = menus[open].items;
    const pickable = (e: Entry) => isCmd(e) && (!e.enabled || e.enabled());
    const onKey = (e: KeyboardEvent) => {
      const step = (dir: 1 | -1) => {
        let i = hot;
        for (let n = 0; n < items.length; n++) {
          i = (i + dir + items.length) % items.length;
          if (pickable(items[i])) return setHot(i);
        }
      };
      if (e.key === "Escape") setOpen(null);
      else if (e.key === "ArrowDown") step(1);
      else if (e.key === "ArrowUp") step(-1);
      else if (e.key === "ArrowRight") {
        setOpen((open + 1) % menus.length);
        setHot(-1);
      } else if (e.key === "ArrowLeft") {
        setOpen((open - 1 + menus.length) % menus.length);
        setHot(-1);
      } else if (e.key === "Enter" && hot >= 0) {
        const it = items[hot];
        if (isCmd(it)) {
          setOpen(null);
          void runCmd(it, onError);
        }
      } else return;
      e.preventDefault();
      e.stopImmediatePropagation();
    };
    // Capture: runs before the shortcut handler and text fields.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, hot, menus, onError]);

  if (isMac) return null;

  return (
    <div className="menubar" ref={bar} role="menubar">
      {menus.map((m, i) => (
        <div key={m.label} className="menubar-slot">
          <button
            className={open === i ? "menubar-item open" : "menubar-item"}
            role="menuitem"
            aria-haspopup="menu"
            aria-expanded={open === i}
            // Keep focus where it was (Edit › Copy needs the text field).
            onMouseDown={(e) => {
              e.preventDefault();
              setOpen(open === i ? null : i);
              setHot(-1);
            }}
            onMouseEnter={() => open !== null && open !== i && (setOpen(i), setHot(-1))}
          >
            {m.label}
          </button>
          {open === i && <MenuList items={m.items} hot={hot} setHot={setHot} close={() => setOpen(null)} onError={onError} />}
        </div>
      ))}
    </div>
  );
}

function MenuList({
  items,
  hot,
  setHot,
  close,
  onError,
  sub,
}: {
  items: Entry[];
  hot: number;
  setHot: (i: number) => void;
  close: () => void;
  onError: (e: unknown) => void;
  sub?: boolean;
}) {
  const [subOpen, setSubOpen] = useState(-1);
  return (
    <div className={sub ? "menu-pop menu-sub" : "menu-pop"} role="menu">
      {items.map((e, i) => {
        if (e === "-") return <div key={`sep${i}`} className="menu-sep" role="separator" />;
        if (isSub(e))
          return (
            <div
              key={e.label}
              className={subOpen === i ? "menu-row has-sub hot" : "menu-row has-sub"}
              onMouseEnter={() => {
                setSubOpen(i);
                setHot(i);
              }}
              onMouseLeave={() => setSubOpen(-1)}
              role="menuitem"
              aria-haspopup="menu"
            >
              <span className="menu-check" />
              <span className="menu-label">{e.label}</span>
              <span className="menu-chev">›</span>
              {subOpen === i && <MenuList items={e.items} hot={-1} setHot={() => undefined} close={close} onError={onError} sub />}
            </div>
          );
        if (!isCmd(e)) return null;
        const disabled = e.enabled ? !e.enabled() : false;
        const checked = e.checked?.();
        return (
          <button
            key={e.id}
            className={hot === i ? "menu-row hot" : "menu-row"}
            role={checked === undefined ? "menuitem" : "menuitemcheckbox"}
            aria-checked={checked}
            disabled={disabled}
            onMouseDown={(ev) => ev.preventDefault()}
            onMouseEnter={() => {
              setHot(i);
              setSubOpen(-1);
            }}
            onClick={() => {
              close();
              void runCmd(e, onError);
            }}
          >
            <span className="menu-check">{checked ? "✓" : ""}</span>
            <span className="menu-label">{e.label}</span>
            {e.accel && <span className="menu-accel">{accelText(e.accel)}</span>}
          </button>
        );
      })}
    </div>
  );
}
