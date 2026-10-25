// The Organizer: what used to be three pages (Home, Watch Folders, Rules)
// as three tabs of one, since they're all about the same job: sorting files
// that arrive.

import { useSyncExternalStore } from "react";
import { createStore } from "../lib/store";
import { Home } from "./Home";
import { Rules } from "./Rules";
import { WatchFolders } from "./WatchFolders";

export type OrganizerTab = "overview" | "watch" | "rules";

const TABS: { id: OrganizerTab; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "watch", label: "Watch folders" },
  { id: "rules", label: "Rules" },
];

const tabStore = createStore<OrganizerTab>("overview");
/** Menus open a tab directly ("Add a watch folder…" shows Watch folders). */
export const showOrganizerTab = (t: OrganizerTab) => tabStore.set(t);

export function Organizer() {
  const tab = useSyncExternalStore(tabStore.subscribe, tabStore.get);
  return (
    <>
      <div className="segmented page-tabs" role="tablist" aria-label="Organizer sections">
        {TABS.map((t) => (
          <button key={t.id} role="tab" aria-selected={tab === t.id} className={tab === t.id ? "active" : ""} onClick={() => tabStore.set(t.id)}>
            {t.label}
          </button>
        ))}
      </div>
      {tab === "overview" && <Home />}
      {tab === "watch" && <WatchFolders />}
      {tab === "rules" && <Rules />}
    </>
  );
}
