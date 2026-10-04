import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { toastError, useToasts } from "../lib/store";

/** Native folder picker; resolves to null when cancelled. */
export async function pickFolder(title: string): Promise<string | null> {
  try {
    const r = await open({ directory: true, multiple: false, title });
    return typeof r === "string" ? r : null;
  } catch (e) {
    toastError(e);
    return null;
  }
}

export function FolderField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (path: string) => void;
}) {
  return (
    <div className="folder-field">
      <span className="folder-label">{label}</span>
      <span className="folder-path" title={value}>{value || "No folder chosen"}</span>
      <button
        onClick={async () => {
          const p = await pickFolder(label);
          if (p) onChange(p);
        }}
      >
        Choose…
      </button>
    </div>
  );
}

export function RevealButton({ path }: { path: string }) {
  return (
    <button className="link" title="Show in folder" onClick={() => api.revealFile(path).catch(toastError)}>
      Show
    </button>
  );
}

export function Toasts() {
  const toasts = useToasts();
  return (
    <div className="toasts" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={t.error ? "toast error" : "toast"}>{t.text}</div>
      ))}
    </div>
  );
}

export function PageHeader({ title, children }: { title: string; children?: React.ReactNode }) {
  return (
    <header className="page-header">
      <h1>{title}</h1>
      <div className="page-actions">{children}</div>
    </header>
  );
}

/** A friendlier empty state: icon, short title, one line of help. */
export function EmptyState({
  icon,
  title,
  children,
}: {
  icon: React.ReactNode;
  title: string;
  children?: React.ReactNode;
}) {
  return (
    <div className="empty empty-big">
      <span className="empty-icon">{icon}</span>
      <div className="empty-title">{title}</div>
      {children && <div>{children}</div>}
    </div>
  );
}
