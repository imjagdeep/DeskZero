// DeskZero's menus: File, Edit, View, Tools, Help.

import { exit } from "@tauri-apps/plugin-process";
import { useEffect, useMemo, useState } from "react";
import { pickFolder } from "./components/Common";
import { PAGES, type Page } from "./components/Sidebar";
import { api, type LabTool } from "./lib/api";
import { openPreview, refreshConfig, refreshStatus, toast, toastError, useConfig, useStatus } from "./lib/store";
import type { Theme } from "./lib/types";
import { canUndo, undoLast } from "./lib/undo";
import { checkAndDownload } from "./lib/updates";
import { focusBrowserSearch } from "./pages/Browse";
import { showOrganizerTab } from "./pages/Organizer";
import { exportRulesFile, importRulesFile } from "./pages/Rules";
import type { MenuDef } from "./shell/menu";
import type { MenuIcon } from "./shell/menuIcons";
import {
  editCommand,
  isCompactSidebar,
  openShellDialog,
  pasteText,
  setZoom,
  textFieldFocused,
  toggleFullscreen,
  toggleSidebar,
  useCompactSidebar,
  zoomStep,
} from "./shell/shell";

const PAGE_ICONS: Partial<Record<Page, MenuIcon>> = { browse: "open", home: "apps", history: "history", settings: "settings" };
const LAB_NAMES: Record<string, string> = { deskmedic: "DeskMedic", deskfolio: "DeskFolio" };

export function useAppMenus(go: (p: Page) => void): MenuDef[] {
  const cfg = useConfig();
  const status = useStatus();
  const compact = useCompactSidebar();
  const [lab, setLab] = useState<LabTool[]>([]);
  useEffect(() => {
    api.desklabTools().then(setLab).catch(toastError);
  }, []);

  return useMemo(() => {
    const settings = cfg?.settings;
    const root = settings?.organize_root ?? null;
    const paused = !!status?.paused;
    const setTheme = async (theme: Theme) => {
      if (!settings) return;
      await api.saveSettings({ ...settings, theme });
      await refreshConfig();
    };
    const themeItem = (t: Theme, label: string) => ({
      id: `theme-${t}`,
      label,
      checked: () => (settings?.theme ?? "system") === t,
      run: () => setTheme(t),
    });

    const file: MenuDef = {
      label: "File",
      items: [
        {
          id: "organize-waiting", icon: "wrap",
          label: "Organize waiting files…",
          accel: "Mod+Shift+O",
          enabled: () => (status?.pending ?? 0) > 0,
          run: async () => {
            go("home");
            openPreview(await api.pendingPlan());
          },
        },
        {
          id: "add-watch", icon: "folder-plus",
          label: "Add a watch folder…",
          enabled: () => !!settings,
          run: async () => {
            const p = await pickFolder("Watch a folder");
            if (!p || !settings) return;
            if (settings.watch_folders.some((w) => w.path === p)) return toast("That folder is already watched");
            await api.saveSettings({ ...settings, watch_folders: [...settings.watch_folders, { path: p, enabled: true }] });
            await refreshConfig();
            showOrganizerTab("watch");
            go("home");
            toast("Watching it now");
          },
        },
        { id: "open-root", icon: "open", label: "Open your DeskZero folder", enabled: () => !!root, run: () => api.openFolder(root ?? "") },
        "-",
        { id: "import-rules", icon: "upload", label: "Import rules…", run: () => importRulesFile(cfg?.rules.length ?? 0) },
        { id: "export-rules", icon: "download", label: "Export rules…", run: exportRulesFile },
        "-",
        {
          id: "pause", icon: "stop",
          label: "Pause watching",
          checked: () => paused,
          run: async () => {
            await api.setPaused(!paused);
            await refreshStatus();
          },
        },
        { id: "settings", icon: "settings", label: "Settings…", accel: "Mod+,", run: () => go("settings") },
        "-",
        // Closing the window keeps DeskZero in the tray; this quits for real.
        { id: "exit", icon: "exit", label: "Exit DeskZero", run: () => exit(0) },
      ],
    };

    const edit: MenuDef = {
      label: "Edit",
      items: [
        {
          id: "undo", icon: "undo",
          label: "Undo",
          accel: "Mod+Z",
          // In a text field: undo typing. Otherwise: the last file action.
          enabled: () => textFieldFocused() || canUndo(),
          run: () => (textFieldFocused() ? editCommand("undo") : undoLast()),
        },
        { id: "redo", icon: "redo", label: "Redo", accel: "Mod+Y", native: "redo", hidden: true, run: () => editCommand("redo") },
        "-",
        { id: "cut", icon: "cut", label: "Cut", accel: "Mod+X", native: "cut", hidden: true, run: () => editCommand("cut") },
        { id: "copy", icon: "copy", label: "Copy", accel: "Mod+C", native: "copy", hidden: true, run: () => editCommand("copy") },
        { id: "paste", icon: "paste", label: "Paste", accel: "Mod+V", native: "paste", hidden: true, run: pasteText },
        { id: "select-all", icon: "select", label: "Select all", accel: "Mod+A", native: "selectAll", hidden: true, run: () => editCommand("selectAll") },
        "-",
        { id: "undo-batch", icon: "history", label: "Undo an organize…", run: () => go("history") },
        // Search lives in the file browser's search box now.
        { id: "find", icon: "search", label: "Find files…", accel: "Mod+F", run: () => (go("browse"), focusBrowserSearch()) },
      ],
    };

    const view: MenuDef = {
      label: "View",
      items: [
        { id: "sidebar", icon: "sidebar", label: "Compact sidebar", accel: "Mod+B", checked: () => isCompactSidebar(), run: toggleSidebar },
        "-",
        { id: "zoom-in", icon: "zoom-in", label: "Zoom in", accel: "Mod+=", run: () => zoomStep(1) },
        { id: "zoom-out", icon: "zoom-out", label: "Zoom out", accel: "Mod+-", run: () => zoomStep(-1) },
        { id: "zoom-reset", icon: "actual-size", label: "Actual size", accel: "Mod+0", run: () => setZoom(1) },
        "-",
        { label: "Theme", icon: "theme", items: [themeItem("system", "Match system"), themeItem("light", "Light"), themeItem("dark", "Dark")] },
        { id: "fullscreen", icon: "fullscreen", label: "Full screen", accel: "F11", run: toggleFullscreen },
      ],
    };

    const tools: MenuDef = {
      label: "Tools",
      items: PAGES.filter((p) => p.page !== "settings").map((p, i) => ({
        id: `go-${p.page}`, icon: PAGE_ICONS[p.page],
        label: p.label,
        accel: i < 9 ? `Mod+${i + 1}` : undefined,
        run: () => go(p.page),
      })),
    };

    const help: MenuDef = {
      label: "Help",
      items: [
        { id: "palette", icon: "command", label: "Search commands…", accel: "Mod+K", run: () => openShellDialog("palette") },
        { id: "shortcuts", icon: "keyboard", label: "Keyboard shortcuts", accel: "F1", run: () => openShellDialog("shortcuts") },
        "-",
        { id: "updates", icon: "update", label: "Check for updates…", run: () => (go("settings"), void checkAndDownload(true)) },
        { id: "whats-new", icon: "news", label: "What's new", run: () => api.openLink("releases") },
        { id: "report", icon: "bug", label: "Report a problem…", run: () => api.openLink("issues") },
        {
          label: "More DeskLab tools",
          icon: "apps",
          items: lab.map((t) => ({
            id: `lab-${t.id}`,
            label: t.installed ? `Open ${LAB_NAMES[t.id] ?? t.id}` : `Get ${LAB_NAMES[t.id] ?? t.id}`,
            run: () => (t.installed ? api.openTool(t.id) : api.openLink(t.id)),
          })),
        },
        "-",
        { id: "about", icon: "info", label: "About DeskZero", run: () => openShellDialog("about") },
      ],
    };
    return [file, edit, view, tools, help];
  }, [cfg, status, go, lab, compact]);
}
