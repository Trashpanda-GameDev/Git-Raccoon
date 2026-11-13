import { useCallback, useRef, useState } from "react";
import { graphPage, loadGraph, locateCommit } from "./api";
import type { GraphCommit } from "./types";

/** Rows are fetched in fixed pages, wherever the view is. */
export const PAGE_SIZE = 500;

/**
 * The open repo's history. `rows` has one slot per commit; slots stay empty
 * until their page is loaded. `version` changes whenever rows arrive.
 */
export type History = {
  snapshot: number;
  total: number;
  width: number;
  branch: string | null;
  path: string;
  rows: (GraphCommit | undefined)[];
  version: number;
};

export function useHistory() {
  const [history, setHistoryState] = useState<History | null>(null);
  const current = useRef<History | null>(null);
  // Page number -> request, for pages loaded or loading in this snapshot.
  const pages = useRef(new Map<number, Promise<void>>());
  // Bumped by every open; results of older opens are dropped.
  const openSeq = useRef(0);

  const setHistory = useCallback((h: History | null) => {
    current.current = h;
    setHistoryState(h);
  }, []);

  /** Opens a repo. Resolves to null if a later open superseded this one. */
  const open = useCallback(
    async (path: string): Promise<History | null> => {
      const seq = ++openSeq.current;
      const page = await loadGraph(path, PAGE_SIZE);
      if (seq !== openSeq.current) return null;
      const rows: (GraphCommit | undefined)[] = new Array(page.total);
      page.commits.forEach((c, i) => (rows[i] = c));
      pages.current = new Map([[0, Promise.resolve()]]);
      const h: History = {
        snapshot: page.snapshot,
        total: page.total,
        width: page.width,
        branch: page.branch,
        path: page.path,
        rows,
        version: 0,
      };
      setHistory(h);
      return h;
    },
    [setHistory],
  );

  const loadPage = useCallback(
    (n: number) => {
      const h = current.current;
      if (!h) return Promise.resolve();
      const existing = pages.current.get(n);
      if (existing) return existing;
      const request = graphPage(h.snapshot, n * PAGE_SIZE, PAGE_SIZE).then(
        (page) => {
          const now = current.current;
          if (!now || now.snapshot !== page.snapshot) return; // reloaded meanwhile
          page.commits.forEach((c, i) => (now.rows[page.offset + i] = c));
          setHistory({ ...now, version: now.version + 1 });
        },
        () => {
          // Allow a retry later (e.g. after a transient error).
          if (current.current === h) pages.current.delete(n);
        },
      );
      pages.current.set(n, request);
      return request;
    },
    [setHistory],
  );

  /** Loads every page overlapping rows `first .. last` (exclusive). */
  const ensureRange = useCallback(
    async (first: number, last: number) => {
      const h = current.current;
      if (!h || h.total === 0) return;
      const from = Math.max(0, Math.floor(first / PAGE_SIZE));
      const to = Math.floor((Math.min(last, h.total) - 1) / PAGE_SIZE);
      const requests = [];
      for (let n = from; n <= to; n++) requests.push(loadPage(n));
      await Promise.all(requests);
    },
    [loadPage],
  );

  /** Finds a commit's row and loads its page. Null if not in the history. */
  const locate = useCallback(
    async (id: string) => {
      const h = current.current;
      if (!h) return null;
      const index = await locateCommit(h.snapshot, id).catch(() => null);
      if (index === null) return null;
      await ensureRange(index, index + 1);
      return index;
    },
    [ensureRange],
  );

  const close = useCallback(() => {
    openSeq.current++;
    pages.current = new Map();
    setHistory(null);
  }, [setHistory]);

  return { history, open, ensureRange, locate, close };
}
