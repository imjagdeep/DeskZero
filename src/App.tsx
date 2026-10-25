import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";
import { useAppMenus } from "./appMenu";
import logo from "./assets/logo.png";
import { Toasts } from "./components/Common";
import { PreviewModal } from "./components/PreviewModal";
import { UpdateBanner } from "./components/UpdateBanner";
import { PAGES, Sidebar, type NavTarget, type Page } from "./components/Sidebar";
import { api } from "./lib/api";
import { fileName } from "./lib/format";
import { openPreview, refreshStatus, startStores, toastError, useConfig } from "./lib/store";
import type { Place } from "./lib/types";
import { checkAndDownload, dailyCheckDue } from "./lib/updates";
import { Browse, type Location } from "./pages/Browse";
import { Cleanup } from "./pages/Cleanup";
import { History } from "./pages/History";
import { Home } from "./pages/Home";
import { Rules } from "./pages/Rules";
import { Settings } from "./pages/Settings";
import { Duplicates, Rename, Search } from "./pages/Tools";
import { WatchFolders } from "./pages/WatchFolders";
import { ShellDialogs } from "./shell/Dialogs";
import { TopBar } from "./shell/TopBar";
import { applySavedZoom } from "./shell/shell";

export default function App() {
  const [page, setPageRaw] = useState<Page>("browse");
  const setPage = useCallback((p: Page) => setPageRaw(p), []);
  const [places, setPlaces] = useState<Place[]>([]);
  const [platform, setPlatform] = useState("");
  // Browser history: back/forward walk this list, like a file manager.
  const [hist, setHist] = useState<{ stack: Location[]; at: number }>({ stack: [], at: -1 });
  const loc = hist.stack[hist.at] ?? null;
  const goLoc = useCallback((l: Location) => {
    setHist((h) => {
      const cur = h.stack[h.at];
      if (cur && cur.path === l.path && cur.kind === l.kind) return h;
      return { stack: [...h.stack.slice(0, h.at + 1), l].slice(-100), at: Math.min(h.at + 1, 99) };
    });
  }, []);
  const menus = useAppMenus(setPage);
  const [version, setVersion] = useState("");
  const cfg = useConfig();
  const theme = cfg?.settings.theme ?? "system";
  const incoming = useRef<string[]>([]);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    startStores();

    // Files from the right-click menu. Explorer starts one launch per
    // selected file, so collect them briefly into one preview.
    const planIncoming = (paths: string[]) => {
      incoming.current.push(...paths);
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(async () => {
        const batch = [...new Set(incoming.current)];
        incoming.current = [];
        setPage("home");
        try {
          openPreview(await api.planPaths(batch));
        } catch (e) {
          toastError(e);
        }
      }, 400);
    };
    api.takeStartupPaths().then((p) => p.length && planIncoming(p)).catch(toastError);

    const unNav = listen<Page>("navigate", (e) => setPage(e.payload));
    const unOrg = listen("organize-now", async () => {
      setPage("home");
      try {
        openPreview(await api.pendingPlan());
      } catch (e) {
        toastError(e);
      }
    });
    const unPend = listen("pending-changed", () => void refreshStatus());
    const unPaths = listen<string[]>("organize-paths", (e) => planIncoming(e.payload));
    return () => {
      for (const un of [unNav, unOrg, unPend, unPaths]) void un.then((f) => f());
    };
  }, []);

  // Automatic updates: soon after start, then daily while DeskZero keeps
  // running in the tray. Downloads quietly, then the banner (and a popup,
  // for when only the tray icon is showing) offers a one-click restart.
  const autoUpdate = !!cfg?.settings.auto_update;
  useEffect(() => {
    if (!autoUpdate) return;
    const run = () => {
      if (!dailyCheckDue()) return;
      void checkAndDownload(false).then((v) => {
        if (v) void api.notifyUpdate(v).catch(() => undefined);
      });
    };
    const first = window.setTimeout(run, 30_000);
    const every = window.setInterval(run, 3 * 3600 * 1000);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(every);
    };
  }, [autoUpdate]);

  // Open on Downloads (or the first standard folder this computer has).
  useEffect(() => {
    api
      .places()
      .then((list) => {
        setPlaces(list);
        const start = list.find((p) => p.id === "downloads") ?? list[0];
        if (start) goLoc({ path: start.path, kind: null });
      })
      .catch(toastError);
    // Each system gets its own look (Finder, File Explorer, GNOME Files).
    api
      .platform()
      .then((os) => {
        setPlatform(os);
        document.documentElement.setAttribute("data-os", os);
      })
      .catch(() => undefined);
  }, [goLoc]);

  const navigate = useCallback(
    (t: NavTarget) => {
      if (t.startsWith("place:")) {
        const pl = places.find((p) => `place:${p.id}` === t);
        if (pl) goLoc({ path: pl.path, kind: null });
        setPage("browse");
      } else setPage(t as Page);
    },
    [places, goLoc, setPage],
  );
  const placeHere = page === "browse" && loc && !loc.kind ? places.find((p) => p.path === loc.path) : undefined;
  const active: NavTarget = placeHere ? `place:${placeHere.id}` : page;
  const folderTitle = loc ? fileName(loc.path.replace(/[\\/]+$/, "")) : "";

  useEffect(() => {
    getVersion().then(setVersion).catch(() => undefined);
    void applySavedZoom().catch(() => undefined);
  }, []);

  // The window title names the page (or folder), like any desktop app.
  useEffect(() => {
    const where = page === "browse" ? folderTitle : (PAGES.find((p) => p.page === page)?.label ?? "");
    void getCurrentWindow()
      .setTitle(where ? `${where} — DeskZero` : "DeskZero")
      .catch(() => undefined);
  }, [page, folderTitle]);

  // "system" leaves the attribute off so the CSS media query decides.
  useEffect(() => {
    if (theme === "system") document.documentElement.removeAttribute("data-theme");
    else document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  return (
    <div className="app-frame">
      <TopBar
        appName="DeskZero"
        logo={logo}
        title={page === "browse" ? folderTitle : PAGES.find((p) => p.page === page)?.label}
        menus={menus}
        onError={toastError}
      />
      <div className="app">
        <Sidebar active={active} places={places} onNavigate={navigate} />
        <main className={page === "browse" ? "content content-browse" : "content"}>
          {page === "browse" && loc && (
            <Browse
              loc={loc}
              go={goLoc}
              back={() => setHist((h) => ({ ...h, at: Math.max(0, h.at - 1) }))}
              forward={() => setHist((h) => ({ ...h, at: Math.min(h.stack.length - 1, h.at + 1) }))}
              canBack={hist.at > 0}
              canForward={hist.at < hist.stack.length - 1}
              platform={platform}
            />
          )}
          {page === "home" && <Home />}
          {page === "watch" && <WatchFolders />}
          {page === "rules" && <Rules />}
          {page === "search" && <Search />}
          {page === "duplicates" && <Duplicates />}
          {page === "rename" && <Rename />}
          {page === "cleanup" && <Cleanup />}
          {page === "history" && <History />}
          {page === "settings" && <Settings />}
        </main>
      </div>
      <PreviewModal />
      <ShellDialogs
        menus={menus}
        onError={toastError}
        about={{
          name: "DeskZero",
          logo,
          version,
          tagline: "Keeps your Downloads and Desktop tidy, finds duplicates, and finds anything fast. Offline.",
          links: [
            { label: "Website", open: () => void api.openLink("site").catch(toastError) },
            { label: "Source code", open: () => void api.openLink("repo").catch(toastError) },
            { label: "What's new", open: () => void api.openLink("releases").catch(toastError) },
            { label: "Report a problem", open: () => void api.openLink("issues").catch(toastError) },
          ],
        }}
      />
      <UpdateBanner app="DeskZero" />
      <Toasts />
    </div>
  );
}
