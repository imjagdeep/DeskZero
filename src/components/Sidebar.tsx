import logo from "../assets/logo.png";
import { useStatus } from "../lib/store";
import { humanSize } from "../lib/format";
import type { Drive, Place, PlaceId } from "../lib/types";
import { SideNav, type NavGroup } from "../shell/SideNav";
import {
  BroomIcon,
  DesktopIcon,
  DocumentIcon,
  DownloadIcon,
  DriveIcon,
  FilmIcon,
  ImageIcon,
  MusicIcon,
  DuplicatesIcon,
  HistoryIcon,
  HomeIcon,
  RenameIcon,
  RulesIcon,
  SearchIcon,
  SettingsIcon,
  SparkleIcon,
  UsbIcon,
  WatchIcon,
} from "./Icons";

export type Page =
  | "browse"
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
  { page: "browse", label: "Files", group: "Places" },
  { page: "home", label: "Organizer", group: "Organize" },
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
  browse: DownloadIcon,
  home: SparkleIcon,
  watch: WatchIcon,
  rules: RulesIcon,
  search: SearchIcon,
  duplicates: DuplicatesIcon,
  rename: RenameIcon,
  cleanup: BroomIcon,
  history: HistoryIcon,
  settings: SettingsIcon,
};

const PLACE_ICONS: Record<PlaceId, (p: { size?: number }) => React.ReactElement> = {
  home: HomeIcon,
  desktop: DesktopIcon,
  documents: DocumentIcon,
  downloads: DownloadIcon,
  pictures: ImageIcon,
  music: MusicIcon,
  videos: FilmIcon,
};

/** A sidebar entry: one of the pages, or a place or drive opened in the browser. */
export type NavTarget = Page | `place:${PlaceId}` | `drive:${string}`;

export function Sidebar({
  active,
  places,
  drives,
  onNavigate,
}: {
  active: NavTarget;
  places: Place[];
  drives: Drive[];
  onNavigate: (t: NavTarget) => void;
}) {
  const status = useStatus();
  const watching = status?.watching.length ?? 0;
  const groups: NavGroup<NavTarget>[] = [
    {
      title: "Places",
      items: places.map((pl) => ({ page: `place:${pl.id}` as const, label: pl.label, Icon: PLACE_ICONS[pl.id], hint: pl.path })),
    },
  ];
  if (drives.length) {
    groups.push({
      title: "Locations",
      items: drives.map((d) => ({
        page: `drive:${d.path}` as const,
        label: d.name,
        Icon: d.removable ? UsbIcon : DriveIcon,
        hint: `${humanSize(d.free)} free of ${humanSize(d.total)}`,
      })),
    });
  }
  for (const p of PAGES) {
    if (p.page === "browse") continue; // the places above are the browser
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
      page={active}
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
