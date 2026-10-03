import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { Toasts } from "./components/Common";
import { PreviewModal } from "./components/PreviewModal";
import { Sidebar, type Page } from "./components/Sidebar";
import { api } from "./lib/api";
import { openPreview, refreshStatus, startStores, toast, toastError, useConfig } from "./lib/store";
import { findUpdate, weeklyCheckDue } from "./lib/updates";
import { Cleanup } from "./pages/Cleanup";
import { History } from "./pages/History";
import { Home } from "./pages/Home";
import { Rules } from "./pages/Rules";
import { Settings } from "./pages/Settings";
import { Duplicates, Rename, Search } from "./pages/Tools";
import { WatchFolders } from "./pages/WatchFolders";

export default function App() {
  const [page, setPage] = useState<Page>("home");
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

  // Weekly update check, only if the user turned it on.
  useEffect(() => {
    if (!cfg?.settings.check_updates_weekly || !weeklyCheckDue()) return;
    findUpdate()
      .then((u) => u && toast(`Super Folder ${u.version} is available: Settings → About to install`))
      .catch(() => {
        // Offline or GitHub unreachable: stay quiet, it's a background check.
      });
  }, [cfg?.settings.check_updates_weekly]);

  // "system" leaves the attribute off so the CSS media query decides.
  useEffect(() => {
    if (theme === "system") document.documentElement.removeAttribute("data-theme");
    else document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  return (
    <div className="app">
      <Sidebar page={page} onNavigate={setPage} />
      <main className="content">
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
      <PreviewModal />
      <Toasts />
    </div>
  );
}
