import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { GraphSvg, RefPills, ROW_HEIGHT, graphWidth, laneColor } from "./components/CommitGraph";
import type { Graph } from "./types";
import "./App.css";

const LAST_REPO_KEY = "git-raccoon:last-repo";

const relativeTime = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 31536000],
  ["month", 2592000],
  ["week", 604800],
  ["day", 86400],
  ["hour", 3600],
  ["minute", 60],
];

function formatAge(seconds: number) {
  const diff = seconds - Date.now() / 1000;
  for (const [unit, size] of UNITS) {
    if (Math.abs(diff) >= size) return relativeTime.format(Math.round(diff / size), unit);
  }
  return "just now";
}

function readLastRepo() {
  try {
    return localStorage.getItem(LAST_REPO_KEY) ?? "";
  } catch {
    return "";
  }
}

function App() {
  const [path, setPath] = useState(readLastRepo);
  const [graph, setGraph] = useState<Graph | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);

  const openRepo = useCallback(async (repoPath: string) => {
    if (!repoPath.trim()) return;
    setError("");
    setLoading(true);
    try {
      const g = await invoke<Graph>("load_graph", { path: repoPath.trim() });
      setGraph(g);
      setSelected(null);
      try {
        localStorage.setItem(LAST_REPO_KEY, repoPath.trim());
      } catch {
        // Remembering the repo is only a convenience.
      }
    } catch (e) {
      setGraph(null);
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    const last = readLastRepo();
    if (last) openRepo(last);
  }, [openRepo]);

  const repoName = graph?.path.replace(/[\\/]+$/, "").split(/[\\/]/).pop();
  const gw = graph ? graphWidth(graph.width) : 0;

  return (
    <div className="app">
      <header className="toolbar">
        <div className="brand">🦝 Git Raccoon</div>
        {graph && (
          <div className="repo-info">
            <span className="repo-name">{repoName}</span>
            <span className="repo-branch">{graph.branch ?? "detached HEAD"}</span>
          </div>
        )}
        <form
          className="open-form"
          onSubmit={(e) => {
            e.preventDefault();
            openRepo(path);
          }}
        >
          <input
            id="repo-path"
            value={path}
            onChange={(e) => setPath(e.currentTarget.value)}
            placeholder="Path to a Git repository…"
            spellCheck={false}
          />
          <button type="submit" disabled={loading}>
            {loading ? "Opening…" : "Open"}
          </button>
        </form>
      </header>

      {error && <div className="error">{error}</div>}

      {graph && (
        <section className="history" style={{ ["--graph-width" as string]: `${gw}px` }}>
          <div className="history-header row-grid">
            <span>Branch / Tag</span>
            <span>Graph</span>
            <span>Commit message</span>
            <span>Author</span>
            <span>Date</span>
            <span>SHA</span>
          </div>
          {graph.commits.length === 0 ? (
            <div className="empty">This repository has no commits yet.</div>
          ) : (
            <div className="history-body">
              <div className="rows" style={{ height: graph.commits.length * ROW_HEIGHT }}>
                <GraphSvg commits={graph.commits} width={graph.width} selected={selected} />
                {graph.commits.map((c) => (
                  <div
                    key={c.id}
                    className={`commit-row row-grid${c.id === selected ? " selected" : ""}`}
                    style={{ height: ROW_HEIGHT, ["--lane-color" as string]: laneColor(c.column) }}
                    onClick={() => setSelected(c.id)}
                  >
                    <span className="refs-cell">
                      <RefPills refs={c.refs} lane={c.column} />
                    </span>
                    <span />
                    <span className="summary" title={c.summary}>
                      {c.summary}
                    </span>
                    <span className="author">{c.author}</span>
                    <span className="date" title={new Date(c.time * 1000).toLocaleString()}>
                      {formatAge(c.time)}
                    </span>
                    <code className="sha">{c.id.slice(0, 7)}</code>
                  </div>
                ))}
              </div>
            </div>
          )}
        </section>
      )}

      {!graph && !error && (
        <div className="welcome">
          <div className="welcome-mark">🦝</div>
          <p>Open a repository to see its history.</p>
        </div>
      )}
    </div>
  );
}

export default App;
