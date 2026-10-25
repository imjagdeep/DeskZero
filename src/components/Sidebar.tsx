import logo from "../assets/logo.png";
import { useStatus } from "../lib/store";
import { fileName, humanSize } from "../lib/format";
import type { Drive, Place, PlaceId } from "../lib/types";
import { SideNav, type NavGroup } from "../shell/SideNav";
import {
  DesktopIcon,
  DocumentIcon,
  DownloadIcon,
  DriveIcon,
  FilmIcon,
  FolderIcon,
  HistoryIcon,
  HomeIcon,
  ImageIcon,
  MusicIcon,
  SettingsIcon,
  SparkleIcon,
  UsbIcon,
} from "./Icons";

export type Page = "browse" | "home" | "history" | "settings";

/** The pages in sidebar order (the Tools menu and Ctrl+1… use it too). */
export const PAGES: { page: Page; label: string; group: string }[] = [
  { page: "browse", label: "Files", group: "Places" },
  { page: "home", label: "Organizer", group: "DeskZero" },
  { page: "history", label: "History", group: "DeskZero" },
  { page: "settings", label: "Settings", group: "DeskZero" },
];

const ICONS: Record<Page, (p: { size?: number }) => React.ReactElement> = {
  browse: DownloadIcon,
  home: SparkleIcon,
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
export type NavTarget = Page | `place:${PlaceId}` | `drive:${string}` | `pin:${string}`;

export function Sidebar({
  active,
  places,
  drives,
  pinned,
  onNavigate,
}: {
  active: NavTarget;
  places: Place[];
  drives: Drive[];
  /** Folders pinned from the browser's right-click menu. */
  pinned: string[];
  onNavigate: (t: NavTarget) => void;
}) {
  const status = useStatus();
  const watching = status?.watching.length ?? 0;
  const groups: NavGroup<NavTarget>[] = [];
  if (pinned.length) {
    groups.push({
      title: "Pinned",
      items: pinned.map((p) => ({ page: `pin:${p}` as const, label: fileName(p.replace(/[\\/]+$/, "")) || p, Icon: FolderIcon, hint: p })),
    });
  }
  groups.push(
    {
      title: "Places",
      items: places.map((pl) => ({ page: `place:${pl.id}` as const, label: pl.label, Icon: PLACE_ICONS[pl.id], hint: pl.path })),
    },
  );
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
