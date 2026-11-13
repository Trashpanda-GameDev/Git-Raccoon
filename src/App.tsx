import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { loadCommitDetails, loadFileDiff, loadGraph, loadRefs } from "./api";
import { GraphSvg, RefPills, ROW_HEIGHT, graphWidth, laneColor } from "./components/CommitGraph";
import { CommitDetailsPane } from "./components/CommitDetailsPane";
import { DiffView, type DiffMode } from "./components/DiffView";
import { Sidebar } from "./components/Sidebar";
import { RepoMenu, Welcome } from "./components/RepoPicker";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { forgetRepo, loadRecent, rememberRepo, repoName as nameOfRepo } from "./recent";
import { formatAge, shortSha } from "./format";
import type { CommitDetails, FileDiff, Graph, Refs } from "./types";
import "./App.css";

const LAST_REPO_KEY = "git-raccoon:last-repo";
const DIFF_MODE_KEY = "git-raccoon:diff-mode";
const SIDEBAR_KEY = "git-raccoon:sidebar";

function readSetting(key: string) {
  try {
    return localStorage.getItem(key) ?? "";
  } catch {
    return "";
  }
}

function writeSetting(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Settings are a convenience; ignore storage failures.
  }
}

/** Loads data for a key, dropping responses that arrive after the key changed. */
function useLoader<T>(key: string | null, load: (key: string) => Promise<T>) {
  const [state, setState] = useState<{ key: string | null; data: T | null; error: string; loading: boolean }>({
    key: null,
    data: null,
    error: "",
    loading: false,
  });
  const latest = useRef<string | null>(null);

  useEffect(() => {
    latest.current = key;
    if (key === null) {
      setState({ key: null, data: null, error: "", loading: false });
      return;
    }
    setState((s) => ({ ...s, key, loading: true, error: "" }));
    load(key).then(
      (data) => latest.current === key && setState({ key, data, error: "", loading: false }),
      (e) => latest.current === key && setState({ key, data: null, error: String(e), loading: false }),
    );
    // `load` is a fresh closure each render; `key` captures everything it reads.
  }, [key]);

  return state;
}

function App() {
  const [recent, setRecent] = useState(loadRecent);
  const [graph, setGraph] = useState<Graph | null>(null);
  const [refs, setRefs] = useState<Refs | null>(null);
  const [showSidebar, setShowSidebar] = useState(() => readSetting(SIDEBAR_KEY) !== "hidden");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [activeFile, setActiveFile] = useState<string | null>(null);
  const [diffMode, setDiffMode] = useState<DiffMode>(() => (readSetting(DIFF_MODE_KEY) === "split" ? "split" : "unified"));
  const bodyRef = useRef<HTMLDivElement>(null);

  const openRepo = useCallback(async (repoPath: string) => {
    const trimmed = repoPath.trim();
    if (!trimmed) return;
    setError("");
    setLoading(true);
    try {
      const [g, r] = await Promise.all([loadGraph(trimmed), loadRefs(trimmed)]);
      setGraph(g);
      setRefs(r);
      setSelected(null);
      setActiveFile(null);
      writeSetting(LAST_REPO_KEY, g.path);
      setRecent(rememberRepo(g.path));
    } catch (e) {
      // Keep whatever repo was open; just report the failure.
      setError(`Could not open ${trimmed}: ${e}`);
    } finally {
      setLoading(false);
    }
  }, []);

  const browse = useCallback(async () => {
    try {
      const picked = await openDialog({ directory: true, multiple: false, title: "Open a Git repository" });
      if (typeof picked === "string") openRepo(picked);
    } catch (e) {
      setError(String(e));
    }
  }, [openRepo]);

  const closeRepo = useCallback(() => {
    setGraph(null);
    setRefs(null);
    setSelected(null);
    setActiveFile(null);
    setError("");
    writeSetting(LAST_REPO_KEY, "");
  }, []);

  const forget = useCallback((p: string) => setRecent(forgetRepo(p)), []);

  useEffect(() => {
    const last = readSetting(LAST_REPO_KEY);
    if (last) openRepo(last);
  }, [openRepo]);

  const repoPath = graph?.path ?? "";
  const details = useLoader<CommitDetails>(selected, (id) => loadCommitDetails(repoPath, id));
  const diffKey = selected && activeFile ? `${selected}\n${activeFile}` : null;
  const diff = useLoader<FileDiff>(diffKey, (key) => {
    const [id, file] = key.split("\n");
    return loadFileDiff(repoPath, id, file);
  });

  const indexById = useMemo(() => new Map(graph?.commits.map((c, i) => [c.id, i]) ?? []), [graph]);

  const select = useCallback((id: string | null) => {
    setSelected(id);
    setActiveFile(null);
    if (id) {
      requestAnimationFrame(() => {
        bodyRef.current?.querySelector(`[data-id="${id}"]`)?.scrollIntoView({ block: "nearest" });
      });
    }
  }, []);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement;
      if (target.closest("input, textarea, [contenteditable]")) return;
      if (e.key === "Escape") {
        if (activeFile) setActiveFile(null);
        else if (selected) select(null);
        return;
      }
      if (!graph || graph.commits.length === 0 || activeFile) return;
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const current = selected ? (indexById.get(selected) ?? -1) : -1;
        const next = Math.min(Math.max(current + (e.key === "ArrowDown" ? 1 : -1), 0), graph.commits.length - 1);
        select(graph.commits[next].id);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [graph, selected, activeFile, indexById, select]);

  const changeDiffMode = (mode: DiffMode) => {
    setDiffMode(mode);
    writeSetting(DIFF_MODE_KEY, mode);
  };

  const gw = graph ? graphWidth(graph.width) : 0;
  // Show the details we have for the current selection, or keep showing the
  // previous commit's (dimmed) while the new one loads.
  const shownDetails = details.data;

  return (
    <div className="app">
      <header className="toolbar">
        <button
          type="button"
          className={`icon-button sidebar-toggle${showSidebar ? " on" : ""}`}
          onClick={() => {
            setShowSidebar((v) => !v);
            writeSetting(SIDEBAR_KEY, showSidebar ? "hidden" : "shown");
          }}
          title={showSidebar ? "Hide sidebar" : "Show sidebar"}
          aria-label="Toggle sidebar"
          aria-pressed={showSidebar}
        >
          ☰
        </button>
        <div className="brand">🦝 Git Raccoon</div>
        {graph && (
          <RepoMenu
            name={nameOfRepo(graph.path)}
            path={graph.path}
            branch={graph.branch}
            recent={recent}
            onBrowse={browse}
            onOpen={openRepo}
            onForget={forget}
            onClose={closeRepo}
          />
        )}
        {loading && <span className="muted toolbar-status">Opening…</span>}
      </header>

      {error && (
        <div className="error dismissable">
          <span>{error}</span>
          <button type="button" className="icon-button" onClick={() => setError("")} aria-label="Dismiss error">
            ×
          </button>
        </div>
      )}

      {graph && (
        <div className="main">
          {showSidebar && <Sidebar refs={refs} selected={selected} onJump={select} />}
          <div className="center">
            <section className="history" style={{ ["--graph-width" as string]: `${gw}px` }}>
              <div className="history-header row-grid">
                <span>Branch / Tag</span>
                <span>Graph</span>
                <span>Commit message</span>
                <span className="col-author">Author</span>
                <span>Date</span>
                <span className="col-sha">SHA</span>
              </div>
              {graph.commits.length === 0 ? (
                <div className="empty">This repository has no commits yet.</div>
              ) : (
                <div className="history-body" ref={bodyRef}>
                  <div className="rows" style={{ height: graph.commits.length * ROW_HEIGHT }}>
                    <GraphSvg commits={graph.commits} width={graph.width} selected={selected} />
                    {graph.commits.map((c) => (
                      <div
                        key={c.id}
                        data-id={c.id}
                        className={`commit-row row-grid${c.id === selected ? " selected" : ""}`}
                        style={{ height: ROW_HEIGHT, ["--lane-color" as string]: laneColor(c.column) }}
                        onClick={() => select(c.id)}
                      >
                        <span className="refs-cell">
                          <RefPills refs={c.refs} lane={c.column} />
                        </span>
                        <span />
                        <span className="summary" title={c.summary}>
                          {c.summary}
                        </span>
                        <span className="author col-author">{c.author}</span>
                        <span className="date" title={new Date(c.time * 1000).toLocaleString()}>
                          {formatAge(c.time)}
                        </span>
                        <code className="sha col-sha">{shortSha(c.id)}</code>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </section>

            {activeFile && (
              <DiffView
                diff={diff.data}
                file={activeFile}
                loading={diff.loading}
                error={diff.error}
                mode={diffMode}
                onModeChange={changeDiffMode}
                onClose={() => setActiveFile(null)}
              />
            )}
          </div>

          {selected && (
            <CommitDetailsPane
              details={shownDetails}
              loading={details.loading}
              error={details.error}
              activeFile={activeFile}
              onOpenFile={setActiveFile}
              onSelectCommit={select}
              isInGraph={(id) => indexById.has(id)}
              onClose={() => select(null)}
            />
          )}
        </div>
      )}

      {!graph && !loading && <Welcome recent={recent} onBrowse={browse} onOpen={openRepo} onForget={forget} />}
    </div>
  );
}

export default App;
