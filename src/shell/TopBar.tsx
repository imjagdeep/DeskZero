// The slim top row: the ☰ menu, plus anything an app puts at the right-hand end
// (the same frame as DeskFolio's tabs row). The app's icon and name are in the
// sidebar's header and the page is in the window title, as in DeskFolio. On a Mac
// the menus live in the system menu bar. Same file in every DeskLab app.

import type { ReactNode } from "react";
import { MenuBar } from "./MenuBar";
import type { MenuDef } from "./menu";

export function TopBar({
  appName,
  menus,
  onError,
  children,
}: {
  appName: string;
  /** Unused since the Daylight look (the sidebar shows the logo); kept so callers don't change. */
  logo?: string;
  /** Unused since the Daylight look (the window title names the page); kept so callers don't change. */
  title?: string;
  menus: MenuDef[];
  onError: (e: unknown) => void;
  /** Anything for the right-hand end (status, a button). */
  children?: ReactNode;
}) {
  return (
    <div className="app-top">
      <MenuBar appName={appName} menus={menus} onError={onError} compact />
      <div className="app-top-end">{children}</div>
    </div>
  );
}
