// A global shortcut the user can change: shows the keys, "Change" records
// the next key combination pressed, "Reset" goes back to the default and
// "Turn off" removes it. Stored as text the Rust side registers
// (keys.rs), e.g. "Control+Shift+Space".

import { useEffect, useState } from "react";

/** null = default, "" = off */
type Value = string | null;

const MODIFIER_CODES = new Set(["ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight", "AltLeft", "AltRight", "MetaLeft", "MetaRight", "OSLeft", "OSRight"]);

/** A keydown → "Control+Alt+KeyK", or an explanation why it can't be used. */
export function comboFromEvent(e: KeyboardEvent): { combo?: string; problem?: string } {
  if (MODIFIER_CODES.has(e.code)) return {};
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Control");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  const fnKey = /^F\d{1,2}$/.test(e.code);
  if (mods.length === 0 && !fnKey) return { problem: "Add Ctrl, Alt, Shift or ⌘, so normal typing isn't taken." };
  return { combo: [...mods, e.code].join("+") };
}

/** "CommandOrControl+Shift+Space" → ["⌘", "Shift", "Space"] (or Ctrl on Windows/Linux). */
export function keyLabels(combo: string, mac: boolean): string[] {
  return combo.split("+").map((part) => {
    switch (part.toLowerCase()) {
      case "commandorcontrol":
      case "cmdorctrl":
        return mac ? "⌘" : "Ctrl";
      case "super":
      case "command":
      case "cmd":
        return mac ? "⌘" : "Win";
      case "control":
      case "ctrl":
        return mac ? "⌃" : "Ctrl";
      case "alt":
      case "option":
        return mac ? "⌥" : "Alt";
      case "shift":
        return "Shift";
    }
    if (/^Key[A-Z]$/.test(part)) return part.slice(3);
    if (/^Digit\d$/.test(part)) return part.slice(5);
    if (/^Numpad\d$/.test(part)) return `Num ${part.slice(6)}`;
    const arrows: Record<string, string> = { ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→" };
    return arrows[part] ?? part;
  });
}

export function ShortcutField({
  label,
  value,
  fallback,
  mac,
  onSave,
}: {
  label: string;
  value: Value;
  /** the default combination */
  fallback: string;
  mac: boolean;
  /** resolves false when the keys couldn't be used */
  onSave: (v: Value) => Promise<boolean>;
}) {
  const [recording, setRecording] = useState(false);
  const [hint, setHint] = useState<string | null>(null);

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
        setRecording(false);
        setHint(null);
        return;
      }
      const { combo, problem } = comboFromEvent(e);
      if (problem) setHint(problem);
      if (!combo) return;
      setRecording(false);
      setHint(null);
      void onSave(combo);
    };
    // Capture phase, so the app's own shortcuts don't act on these keys.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, onSave]);

  const current = value ?? fallback;
  const off = current === "";
  return (
    <div className="shortcut-row">
      <span className="shortcut-name">{label}</span>
      <span className="shortcut-keys" aria-live="polite">
        {recording ? (
          <span className="muted">{hint ?? "Press the new keys… (Esc to cancel)"}</span>
        ) : off ? (
          <span className="muted">Off</span>
        ) : (
          keyLabels(current, mac).map((k, i) => (
            <span key={i}>
              {i > 0 && "+"}
              <kbd>{k}</kbd>
            </span>
          ))
        )}
      </span>
      <span className="shortcut-actions">
        <button className="link" onClick={() => (setHint(null), setRecording(!recording))}>
          {recording ? "Cancel" : "Change"}
        </button>
        {value !== null && !recording && (
          <button className="link" onClick={() => void onSave(null)}>
            Reset
          </button>
        )}
        {!off && !recording && (
          <button className="link" onClick={() => void onSave("")}>
            Turn off
          </button>
        )}
      </span>
    </div>
  );
}
