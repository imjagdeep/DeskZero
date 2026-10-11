import { useState } from "react";
import { installNow, useUpdate } from "../lib/updates";

/** "Version X is ready" bar with the one-click restart. */
export function UpdateBanner({ app }: { app: string }) {
  const st = useUpdate();
  const [later, setLater] = useState<string | null>(null);
  if (st.kind === "installing") {
    return (
      <div className="update-banner" role="status">
        <div className="spinner small" aria-hidden="true" />
        <span>Installing {app} {st.version}… it restarts by itself.</span>
      </div>
    );
  }
  if (st.kind !== "ready" || later === st.version) return null;
  return (
    <div className="update-banner" role="status">
      <span>
        <strong>{app} {st.version}</strong> is downloaded and ready.
      </span>
      <div className="update-banner-actions">
        <button onClick={() => setLater(st.version)}>Later</button>
        <button className="primary" onClick={() => void installNow()}>
          Restart to update
        </button>
      </div>
    </div>
  );
}
