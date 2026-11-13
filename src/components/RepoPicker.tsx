import { useEffect, useRef, useState } from "react";
import type { RecentRepo } from "../recent";
import { formatAge } from "../format";

type RecentListProps = {
  recent: RecentRepo[];
  current?: string;
  onOpen: (path: string) => void;
  onForget: (path: string) => void;
};

function RecentList({ recent, current, onOpen, onForget }: RecentListProps) {
  return (
    <ul className="recent-list">
      {recent.map((r) => (
        <li key={r.path} className={r.path === current ? "current" : ""}>
          <button type="button" className="recent-open" onClick={() => onOpen(r.path)} title={r.path}>
            <span className="recent-name">{r.name}</span>
            <span className="recent-path muted">{r.path}</span>
          </button>
          <span className="recent-age muted">{formatAge(r.openedAt / 1000)}</span>
          <button type="button" className="icon-button" onClick={() => onForget(r.path)} title="Remove from recent" aria-label={`Remove ${r.name} from recent`}>
            ×
          </button>
        </li>
      ))}
    </ul>
  );
}

type WelcomeProps = {
  recent: RecentRepo[];
  onBrowse: () => void;
  onOpen: (path: string) => void;
  onForget: (path: string) => void;
};

export function Welcome({ recent, onBrowse, onOpen, onForget }: WelcomeProps) {
  return (
    <div className="welcome">
      <div className="welcome-mark">🦝</div>
      <h1>Git Raccoon</h1>
      <button type="button" className="primary-button" onClick={onBrowse}>
        Open a repository…
      </button>
      {recent.length > 0 && (
        <section className="welcome-recent">
          <h2>Recent</h2>
          <RecentList recent={recent} onOpen={onOpen} onForget={onForget} />
        </section>
      )}
    </div>
  );
}

type RepoMenuProps = {
  name: string;
  path: string;
  branch: string | null;
  recent: RecentRepo[];
  onBrowse: () => void;
  onOpen: (path: string) => void;
  onForget: (path: string) => void;
  onClose: () => void;
};

/** The repo name in the toolbar; opens a menu to switch or close repos. */
export function RepoMenu({ name, path, branch, recent, onBrowse, onOpen, onForget, onClose }: RepoMenuProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [open]);

  const act = (fn: () => void) => () => {
    setOpen(false);
    fn();
  };

  return (
    <div className="repo-menu" ref={ref}>
      <button type="button" className="repo-button" onClick={() => setOpen((v) => !v)} aria-expanded={open} title={path}>
        <span className="repo-name">{name}</span>
        <span className="caret open">▸</span>
      </button>
      <span className="repo-branch">{branch ?? "detached HEAD"}</span>
      {open && (
        <div className="menu" role="menu">
          <button type="button" className="menu-item" onClick={act(onBrowse)}>
            Open repository…
          </button>
          <button type="button" className="menu-item" onClick={act(onClose)}>
            Close repository
          </button>
          {recent.length > 0 && (
            <>
              <div className="menu-label">Recent</div>
              <RecentList recent={recent} current={path.replace(/[\\/]+$/, "")} onOpen={(p) => act(() => onOpen(p))()} onForget={onForget} />
            </>
          )}
        </div>
      )}
    </div>
  );
}
