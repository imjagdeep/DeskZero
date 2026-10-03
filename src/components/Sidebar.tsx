import { useStatus } from "../lib/store";

export type Page = "home" | "watch" | "rules" | "search" | "duplicates" | "rename" | "history" | "settings";

const ITEMS: { page: Page; icon: string; label: string }[] = [
  { page: "home", icon: "🏠", label: "Home" },
  { page: "watch", icon: "📥", label: "Watch Folders" },
  { page: "rules", icon: "📋", label: "Rules" },
  { page: "search", icon: "🔍", label: "Search" },
  { page: "duplicates", icon: "⧉", label: "Duplicates" },
  { page: "rename", icon: "✎", label: "Rename" },
  { page: "history", icon: "♻", label: "History & Undo" },
  { page: "settings", icon: "⚙", label: "Settings" },
];

export function Sidebar({ page, onNavigate }: { page: Page; onNavigate: (p: Page) => void }) {
  const status = useStatus();
  return (
    <nav className="sidebar">
      <div className="brand">Super Folder</div>
      {ITEMS.map((it) => (
        <button
          key={it.page}
          className={page === it.page ? "nav-item active" : "nav-item"}
          onClick={() => onNavigate(it.page)}
        >
          <span className="nav-icon" aria-hidden="true">{it.icon}</span>
          {it.label}
          {it.page === "home" && status && status.pending > 0 && <span className="count">{status.pending}</span>}
        </button>
      ))}
      <div className="sidebar-foot">
        {status?.paused ? (
          <span>⏸ Monitoring paused</span>
        ) : status && status.watching.length > 0 ? (
          <span>● Watching {status.watching.length} folder{status.watching.length === 1 ? "" : "s"}</span>
        ) : (
          <span className="muted">Not watching</span>
        )}
      </div>
    </nav>
  );
}
