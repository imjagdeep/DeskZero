import logo from "../assets/logo.png";
import { useStatus } from "../lib/store";
import {
  BroomIcon,
  DuplicatesIcon,
  HistoryIcon,
  HomeIcon,
  RenameIcon,
  RulesIcon,
  SearchIcon,
  SettingsIcon,
  WatchIcon,
} from "./Icons";

export type Page =
  | "home"
  | "watch"
  | "rules"
  | "search"
  | "duplicates"
  | "rename"
  | "cleanup"
  | "history"
  | "settings";

type Item = { page: Page; label: string; Icon: (p: { size?: number }) => React.ReactElement };

const GROUPS: { title: string; items: Item[] }[] = [
  {
    title: "Organize",
    items: [
      { page: "home", label: "Home", Icon: HomeIcon },
      { page: "watch", label: "Watch Folders", Icon: WatchIcon },
      { page: "rules", label: "Rules", Icon: RulesIcon },
    ],
  },
  {
    title: "Tools",
    items: [
      { page: "search", label: "Search", Icon: SearchIcon },
      { page: "duplicates", label: "Duplicates", Icon: DuplicatesIcon },
      { page: "rename", label: "Rename", Icon: RenameIcon },
      { page: "cleanup", label: "Cleanup", Icon: BroomIcon },
    ],
  },
  {
    title: "App",
    items: [
      { page: "history", label: "History & Undo", Icon: HistoryIcon },
      { page: "settings", label: "Settings", Icon: SettingsIcon },
    ],
  },
];

export function Sidebar({ page, onNavigate }: { page: Page; onNavigate: (p: Page) => void }) {
  const status = useStatus();
  const watching = status?.watching.length ?? 0;
  return (
    <nav className="sidebar">
      <div className="brand">
        <img src={logo} alt="" className="brand-logo" />
        <span>DeskZero</span>
      </div>
      {GROUPS.map((g) => (
        <div className="nav-group" key={g.title}>
          <div className="nav-title">{g.title}</div>
          {g.items.map(({ page: p, label, Icon }) => (
            <button
              key={p}
              className={page === p ? "nav-item active" : "nav-item"}
              aria-current={page === p ? "page" : undefined}
              onClick={() => onNavigate(p)}
            >
              <Icon size={18} />
              <span className="nav-label">{label}</span>
              {p === "home" && status && status.pending > 0 && <span className="count">{status.pending}</span>}
            </button>
          ))}
        </div>
      ))}
      <div className="sidebar-foot">
        <span className={status?.paused ? "dot paused" : watching > 0 ? "dot on" : "dot"} />
        {status?.paused
          ? "Monitoring paused"
          : watching > 0
            ? `Watching ${watching} folder${watching === 1 ? "" : "s"}`
            : "Not watching"}
      </div>
    </nav>
  );
}
