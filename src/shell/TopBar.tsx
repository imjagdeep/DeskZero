// The slim top row: ☰ menu, the app's icon and name, and the page you're on
// (the same frame as DeskFolio's document tabs row). On a Mac the menus live
// in the system menu bar, so only the name and page show here.
// Same file in every DeskLab app.

import type { ReactNode } from "react";
import { MenuBar } from "./MenuBar";
import type { MenuDef } from "./menu";

export function TopBar({
  appName,
  logo,
  title,
  menus,
  onError,
  children,
}: {
  appName: string;
  logo: string;
  /** The page you're on ("History"); hidden when it's the app's home. */
  title?: string;
  menus: MenuDef[];
  onError: (e: unknown) => void;
  /** Anything for the right-hand end (status, a button). */
  children?: ReactNode;
}) {
  return (
    <div className="app-top">
      <MenuBar appName={appName} menus={menus} onError={onError} compact />
      <img className="app-top-logo" src={logo} alt="" />
      <span className="app-top-name">{appName}</span>
      {title && title !== appName && (
        <>
          <span className="app-top-sep" aria-hidden="true">
            /
          </span>
          <span className="app-top-title">{title}</span>
        </>
      )}
      <div className="app-top-end">{children}</div>
    </div>
  );
}
