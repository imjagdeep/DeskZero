import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { Toasts } from "./components/Common";
import { Sidebar, type Page } from "./components/Sidebar";
import { refreshStatus, startStores, useConfig } from "./lib/store";
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

  useEffect(() => {
    startStores();
    const unNav = listen<Page>("navigate", (e) => setPage(e.payload));
    const unOrg = listen("organize-now", () => setPage("home"));
    const unPend = listen("pending-changed", () => void refreshStatus());
    return () => {
      for (const un of [unNav, unOrg, unPend]) void un.then((f) => f());
    };
  }, []);

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
        {page === "history" && <History />}
        {page === "settings" && <Settings />}
      </main>
      <Toasts />
    </div>
  );
}
