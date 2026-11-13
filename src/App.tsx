import { useCallback, useEffect, useMemo, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { loadCommitDetails, loadFileDiff, loadRefs } from "./api";
import { CommitDetailsPane } from "./components/CommitDetailsPane";
import { DiffView, type DiffMode } from "./components/DiffView";
import { HistoryView } from "./components/HistoryView";
import { RepoMenu, Welcome } from "./components/RepoPicker";
import { Sidebar } from "./components/Sidebar";
import { forgetRepo, loadRecent, rememberRepo, repoName } from "./recent";
import { DIFF_MODE_KEY, LAST_REPO_KEY, SIDEBAR_KEY, readSetting, writeSetting } from "./settings";
import type { CommitDetails, FileDiff, Refs } from "./types";
import { useHistory } from "./useHistory";
import { useLoader } from "./useLoader";
import "./App.css";

function App() {
  const { history, open, ensureRange, locate, close } = useHistory();
  const [refs, setRefs] = useState<Refs | null>(null);
  const [recent, setRecent] = useState(loadRecent);
  const [showSidebar, setShowSidebar] = useState(() => readSetting(SIDEBAR_KEY) !== "hidden");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [scrollTarget, setScrollTarget] = useState<{ index: number; nonce: number } | null>(null);
  const [activeFile, setActiveFile] = useState<string | null>(null);
  const [diffMode, setDiffMode] = useState<DiffMode>(() => (readSetting(DIFF_MODE_KEY) === "split" ? "split" : "unified"));

  const openRepo = useCallback(
    async (repoPath: string) => {
      const trimmed = repoPath.trim();
      if (!trimmed) return;
      setError("");
      setLoading(true);
      try {
        const [h, r] = await Promise.all([open(trimmed), loadRefs(trimmed)]);
        if (!h) return; // another repo was opened meanwhile
        setRefs(r);
        setSelected(null);
        setActiveFile(null);
        writeSetting(LAST_REPO_KEY, h.path);
        setRecent(rememberRepo(h.path));
      } catch (e) {
        // Keep whatever repo was open; just report the failure.
        setError(`Could not open ${trimmed}: ${e}`);
      } finally {
        setLoading(false);
      }
    },
    [open],
  );

  const browse = useCallback(async () => {
    try {
      const picked = await openDialog({ directory: true, multiple: false, title: "Open a Git repository" });
      if (typeof picked === "string") openRepo(picked);
    } catch (e) {
      setError(String(e));
    }
  }, [openRepo]);

  const closeRepo = useCallback(() => {
    close();
    setRefs(null);
    setSelected(null);
    setActiveFile(null);
    setError("");
    writeSetting(LAST_REPO_KEY, "");
  }, [close]);

  const forget = useCallback((p: string) => setRecent(forgetRepo(p)), []);

  useEffect(() => {
    const last = readSetting(LAST_REPO_KEY);
    if (last) openRepo(last);
  }, [openRepo]);

  const repoPath = history?.path ?? "";
  const details = useLoader<CommitDetails>(selected, (id) => loadCommitDetails(repoPath, id));
  const diffKey = selected && activeFile ? `${selected}\n${activeFile}` : null;
  const diff = useLoader<FileDiff>(diffKey, (key) => {
    const [id, file] = key.split("\n");
    return loadFileDiff(repoPath, id, file);
  });

  // Rows of the loaded commits, rebuilt as pages arrive.
  const indexById = useMemo(() => {
    const map = new Map<string, number>();
    history?.rows.forEach((c, i) => c && map.set(c.id, i));
    return map;
  }, [history?.snapshot, history?.version]);

  /** Selects a commit and scrolls its row (if any) into view. */
  const selectRow = useCallback((id: string, index: number | null) => {
    setSelected(id);
    setActiveFile(null);
    if (index !== null) setScrollTarget({ index, nonce: Math.random() });
  }, []);

  const deselect = useCallback(() => {
    setSelected(null);
    setActiveFile(null);
  }, []);

  /** Selects any commit by id, loading history up to its row if needed. */
  const jumpTo = useCallback(
    async (id: string) => {
      selectRow(id, indexById.get(id) ?? null);
      if (!indexById.has(id)) {
        const index = await locate(id);
        if (index !== null) setScrollTarget({ index, nonce: Math.random() });
      }
    },
    [indexById, locate, selectRow],
  );

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement;
      if (target.closest("input, textarea, [contenteditable]")) return;
      if (e.key === "Escape") {
        if (activeFile) setActiveFile(null);
        else if (selected) deselect();
        return;
      }
      if (!history || history.total === 0 || activeFile) return;
      const steps: Record<string, number> = { ArrowDown: 1, ArrowUp: -1, PageDown: 20, PageUp: -20 };
      const step = steps[e.key];
      if (step === undefined) return;
      e.preventDefault();
      const current = selected ? (indexById.get(selected) ?? -1) : -1;
      const next = Math.min(Math.max(current + step, 0), history.total - 1);
      const row = history.rows[next];
      if (row) selectRow(row.id, next);
      else ensureRange(next, next + 1).then(() => {
        const loaded = history.rows[next];
        if (loaded) selectRow(loaded.id, next);
      });
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [history, selected, activeFile, indexById, selectRow, deselect, ensureRange]);

  const changeDiffMode = (mode: DiffMode) => {
    setDiffMode(mode);
    writeSetting(DIFF_MODE_KEY, mode);
  };

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
        {history && (
          <RepoMenu
            name={repoName(history.path)}
            path={history.path}
            branch={history.branch}
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

      {history && (
        <div className="main">
          {showSidebar && <Sidebar refs={refs} selected={selected} onJump={jumpTo} />}
          <div className="center">
            <HistoryView
              history={history}
              selected={selected}
              scrollTarget={scrollTarget}
              onSelect={selectRow}
              onNeedRows={ensureRange}
            />

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
              details={details.data}
              loading={details.loading}
              error={details.error}
              activeFile={activeFile}
              onOpenFile={setActiveFile}
              onSelectCommit={jumpTo}
              onClose={deselect}
            />
          )}
        </div>
      )}

      {!history && !loading && <Welcome recent={recent} onBrowse={browse} onOpen={openRepo} onForget={forget} />}
    </div>
  );
}

export default App;
