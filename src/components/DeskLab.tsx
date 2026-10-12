// "More free tools": the other DeskLab apps, opened directly when installed,
// otherwise their download page. Same file in DeskZero, DeskMedic and DeskFolio.

import { useEffect, useState } from "react";
import { api, type LabTool, type LabToolId } from "../lib/api";
import { toastError } from "../lib/store";

const ABOUT: Record<LabToolId, { name: string; blurb: string }> = {
  deskzero: { name: "DeskZero", blurb: "Keeps your folders organized and finds anything fast. Windows, macOS and Linux." },
  deskmedic: { name: "DeskMedic", blurb: "Windows helpdesk toolkit: health check, disk map, cleanup and one-click remedies." },
  deskfolio: { name: "DeskFolio", blurb: "Offline PDF tool: view, sign, compress, convert and OCR. Windows, macOS and Linux." },
};

export function DeskLabTools({ own }: { own: LabToolId }) {
  const [tools, setTools] = useState<LabTool[]>(() =>
    (Object.keys(ABOUT) as LabToolId[]).filter((id) => id !== own).map((id) => ({ id, installed: false })),
  );
  useEffect(() => {
    api.desklabTools().then(setTools).catch(toastError);
  }, []);

  return (
    <>
      <div className="muted small">DeskLab: more free tools by the same developer</div>
      {tools.map((t) => (
        <button
          key={t.id}
          className="tool-link"
          onClick={() => (t.installed ? api.openTool(t.id) : api.openLink(t.id)).catch(toastError)}
        >
          <strong>{ABOUT[t.id].name}</strong>
          <span className="muted small">{ABOUT[t.id].blurb}</span>
          <span className="muted small">{t.installed ? `Installed: click to open ${ABOUT[t.id].name}` : "Not installed: click for the free download"}</span>
        </button>
      ))}
    </>
  );
}
