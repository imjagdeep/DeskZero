import logo from "../assets/logo.png";
import { useStatus } from "../lib/store";
import { SideNav, type NavGroup } from "../shell/SideNav";
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

/** The pages in sidebar order (the Tools menu and Ctrl+1… use it too). */
export const PAGES: { page: Page; label: string; group: string }[] = [
  { page: "home", label: "Home", group: "Organize" },
  { page: "watch", label: "Watch Folders", group: "Organize" },
  { page: "rules", label: "Rules", group: "Organize" },
  { page: "search", label: "Search", group: "Tools" },
  { page: "duplicates", label: "Duplicates", group: "Tools" },
  { page: "rename", label: "Rename", group: "Tools" },
  { page: "cleanup", label: "Cleanup", group: "Tools" },
  { page: "history", label: "History & Undo", group: "App" },
  { page: "settings", label: "Settings", group: "App" },
];

const ICONS: Record<Page, (p: { size?: number }) => React.ReactElement> = {
  home: HomeIcon,
  watch: WatchIcon,
  rules: RulesIcon,
  search: SearchIcon,
  duplicates: DuplicatesIcon,
  rename: RenameIcon,
  cleanup: BroomIcon,
  history: HistoryIcon,
  settings: SettingsIcon,
};

export function Sidebar({ page, onNavigate }: { page: Page; onNavigate: (p: Page) => void }) {
  const status = useStatus();
  const watching = status?.watching.length ?? 0;
  const groups: NavGroup<Page>[] = [];
  for (const p of PAGES) {
    let g = groups.find((x) => x.title === p.group);
    if (!g) groups.push((g = { title: p.group, items: [] }));
    const badge = p.page === "home" && status && status.pending > 0 ? <span className="count">{status.pending}</span> : undefined;
    g.items.push({ page: p.page, label: p.label, Icon: ICONS[p.page], badge });
  }
  const state = status?.paused ? "Monitoring paused" : watching > 0 ? `Watching ${watching} folder${watching === 1 ? "" : "s"}` : "Not watching";
  return (
    <SideNav
      appName="DeskZero"
      logo={logo}
      groups={groups}
      page={page}
      onNavigate={onNavigate}
      footTitle={state}
      foot={
        <>
          <span className={status?.paused ? "dot paused" : watching > 0 ? "dot on" : "dot"} />
          <span className="ellipsis">{state}</span>
        </>
      }
    />
  );
}
