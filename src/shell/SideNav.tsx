// The sidebar: full (icon + name) or compact (icons only, names as
// tooltips). Ctrl/Cmd+B or the button at the top switches; narrow windows go
// compact by themselves. Same file in DeskZero, DeskMedic and DeskFolio.

import { useEffect, useState } from "react";
import { useCompactSidebar, toggleSidebar } from "./shell";

export interface NavItem<P extends string> {
  page: P;
  label: string;
  Icon: (p: { size?: number }) => React.ReactElement;
  disabled?: boolean;
  /** Why it is disabled, or extra help. */
  hint?: string;
  /** A count or dot after the name (shown on the icon when compact). */
  badge?: React.ReactNode;
}
export interface NavGroup<P extends string> {
  title: string;
  items: NavItem<P>[];
}

const NARROW = 860;

export function SideNav<P extends string>({
  appName,
  logo,
  groups,
  page,
  onNavigate,
  foot,
  footTitle,
}: {
  appName: string;
  logo: string;
  groups: NavGroup<P>[];
  page: P;
  onNavigate: (p: P) => void;
  /** Status line at the bottom; its first child (a dot) stays when compact. */
  foot: React.ReactNode;
  footTitle?: string;
}) {
  const chosen = useCompactSidebar();
  const [narrow, setNarrow] = useState(() => window.innerWidth < NARROW);
  useEffect(() => {
    const onResize = () => setNarrow(window.innerWidth < NARROW);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  const compact = chosen || narrow;

  return (
    <nav className={compact ? "sidebar compact" : "sidebar"} aria-label={`${appName} pages`}>
      <div className="brand">
        <img src={logo} alt="" className="brand-logo" />
        <span className="brand-name">{appName}</span>
        <button
          className="side-toggle"
          onClick={toggleSidebar}
          disabled={narrow && !chosen}
          title={compact ? "Show names (Ctrl+B)" : "Icons only (Ctrl+B)"}
          aria-label={compact ? "Expand sidebar" : "Collapse sidebar"}
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
            <rect x="1.5" y="2.5" width="13" height="11" rx="2.2" />
            <path d="M6 2.5v11" />
            <path d={compact ? "M9.2 6.2 11 8l-1.8 1.8" : "M11 6.2 9.2 8 11 9.8"} strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>
      </div>
      {groups.map((g) => (
        <div className="nav-group" key={g.title}>
          <div className="nav-title">{g.title}</div>
          {g.items.map(({ page: p, label, Icon, disabled, hint, badge }) => (
            <button
              key={p}
              className={page === p ? "nav-item active" : "nav-item"}
              aria-current={page === p ? "page" : undefined}
              aria-label={compact ? label : undefined}
              disabled={disabled}
              title={hint}
              data-tip={compact ? label : undefined}
              onClick={() => onNavigate(p)}
            >
              <Icon size={18} />
              <span className="nav-label">{label}</span>
              {badge}
            </button>
          ))}
        </div>
      ))}
      <div className="sidebar-foot" title={footTitle}>
        {foot}
      </div>
    </nav>
  );
}
