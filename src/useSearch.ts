import { useEffect, useMemo, useRef, useState } from "react";
import { searchCommits } from "./api";
import type { SearchField } from "./types";

/** Stop asking for more once this many commits match. */
export const MAX_MATCHES = 10_000;
const DEBOUNCE_MS = 250;

export type SearchState = {
  query: string;
  field: SearchField;
  /** Matching rows found so far, in history order. */
  rows: number[];
  /** Rows searched so far, of `total`. */
  searched: number;
  total: number;
  searching: boolean;
  truncated: boolean;
  error: string;
};

/**
 * Searches the history behind `snapshot` as the query changes, collecting
 * matches slice by slice so they appear while the search runs.
 */
export function useSearch(snapshot: number | null, total: number) {
  const [query, setQuery] = useState("");
  const [field, setField] = useState<SearchField>("text");
  const [result, setResult] = useState<Omit<SearchState, "query" | "field" | "total">>({
    rows: [],
    searched: 0,
    searching: false,
    truncated: false,
    error: "",
  });
  const seq = useRef(0);

  useEffect(() => {
    const mine = ++seq.current;
    const q = query.trim();
    if (snapshot === null || !q) {
      setResult({ rows: [], searched: 0, searching: false, truncated: false, error: "" });
      return;
    }
    setResult({ rows: [], searched: 0, searching: true, truncated: false, error: "" });
    const timer = setTimeout(async () => {
      let start = 0;
      let rows: number[] = [];
      try {
        for (;;) {
          const r = await searchCommits(snapshot, q, field, start);
          if (mine !== seq.current || r.cancelled) return;
          rows = rows.concat(r.rows);
          const truncated = rows.length > MAX_MATCHES;
          if (truncated) rows = rows.slice(0, MAX_MATCHES);
          const done = r.next === null || truncated;
          setResult({ rows, searched: r.next ?? total, searching: !done, truncated, error: "" });
          if (done) return;
          start = r.next!;
        }
      } catch (e) {
        if (mine === seq.current) setResult((s) => ({ ...s, searching: false, error: String(e) }));
      }
    }, DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [snapshot, query, field, total]);

  const matches = useMemo(() => new Set(result.rows), [result.rows]);
  const state: SearchState = { query, field, total, ...result };
  return { search: state, matches, setQuery, setField };
}
