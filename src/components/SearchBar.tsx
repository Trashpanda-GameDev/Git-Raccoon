import { forwardRef } from "react";
import type { SearchField } from "../types";
import type { SearchState } from "../useSearch";

type Props = {
  search: SearchState;
  /** Position of the selected commit among the matches, if it is one. */
  currentMatch: number | null;
  onQuery: (q: string) => void;
  onField: (f: SearchField) => void;
  onStep: (direction: 1 | -1) => void;
};

export const SearchBar = forwardRef<HTMLInputElement, Props>(function SearchBar(
  { search, currentMatch, onQuery, onField, onStep },
  ref,
) {
  const { query, field, rows, searching, searched, total, truncated, error } = search;
  const active = query.trim() !== "";
  const percent = total ? Math.floor((searched / total) * 100) : 0;

  let status = "";
  if (error) status = "Search failed";
  else if (active) {
    const count = `${rows.length.toLocaleString()}${truncated ? "+" : ""}`;
    const position = currentMatch !== null ? `${(currentMatch + 1).toLocaleString()} of ` : "";
    status = rows.length === 0 && !searching ? "No matches" : `${position}${count} ${rows.length === 1 ? "match" : "matches"}`;
    if (searching) status += ` · ${percent}%`;
  }

  return (
    <div className={`search-bar${active ? " active" : ""}`} role="search">
      <span className="search-icon" aria-hidden="true">
        ⌕
      </span>
      <input
        ref={ref}
        value={query}
        onChange={(e) => onQuery(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            onStep(e.shiftKey ? -1 : 1);
          } else if (e.key === "Escape") {
            onQuery("");
            e.currentTarget.blur();
          }
        }}
        placeholder={field === "text" ? "Search messages, authors, SHAs…" : "Search changed file paths…"}
        spellCheck={false}
        aria-label="Search commits"
      />
      <div className="segmented" role="group" aria-label="Search in">
        {(["text", "path"] as const).map((f) => (
          <button key={f} type="button" className={field === f ? "on" : ""} aria-pressed={field === f} onClick={() => onField(f)}>
            {f === "text" ? "Message" : "File"}
          </button>
        ))}
      </div>
      <span className="search-status muted" title={error || undefined} aria-live="polite">
        {status}
      </span>
      <button type="button" className="icon-button" onClick={() => onStep(-1)} disabled={rows.length === 0} title="Previous match (Shift+Enter)" aria-label="Previous match">
        ↑
      </button>
      <button type="button" className="icon-button" onClick={() => onStep(1)} disabled={rows.length === 0} title="Next match (Enter)" aria-label="Next match">
        ↓
      </button>
      {active && (
        <button type="button" className="icon-button" onClick={() => onQuery("")} title="Clear search (Esc)" aria-label="Clear search">
          ×
        </button>
      )}
    </div>
  );
});
