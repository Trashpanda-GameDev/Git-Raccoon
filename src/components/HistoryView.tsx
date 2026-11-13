import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { GraphSvg, RefPills, ROW_HEIGHT, graphColumnWidth, laneColor } from "./CommitGraph";
import { formatAge, shortSha } from "../format";
import type { History } from "../useHistory";

/** Rows drawn above and below the visible ones. */
const OVERSCAN = 20;

type Props = {
  history: History;
  selected: string | null;
  /** Bump `nonce` to scroll row `index` into view. */
  scrollTarget: { index: number; nonce: number } | null;
  onSelect: (id: string, index: number) => void;
  /** Called with the row range `first .. last` that should be loaded. */
  onNeedRows: (first: number, last: number) => void;
  /** Rows matching the active search, or null when not searching. */
  matches: Set<number> | null;
};

export function HistoryView({ history, selected, scrollTarget, onSelect, onNeedRows, matches }: Props) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(600);
  const { rows: commits, total } = history;

  useLayoutEffect(() => {
    const body = bodyRef.current;
    if (!body) return;
    const observer = new ResizeObserver(() => setHeight(body.clientHeight));
    observer.observe(body);
    setHeight(body.clientHeight);
    return () => observer.disconnect();
  }, []);

  // Another repo starts at the top; a refresh of the same one stays put.
  useEffect(() => {
    if (bodyRef.current) bodyRef.current.scrollTop = 0;
    setScrollTop(0);
  }, [history.path]);

  useEffect(() => {
    const body = bodyRef.current;
    if (!scrollTarget || !body) return;
    const top = scrollTarget.index * ROW_HEIGHT;
    if (top < body.scrollTop) body.scrollTop = top;
    else if (top + ROW_HEIGHT > body.scrollTop + body.clientHeight) {
      // Far jumps land mid-screen; single steps just reveal the row.
      const far = top > body.scrollTop + body.clientHeight + ROW_HEIGHT * 2;
      body.scrollTop = far ? top - body.clientHeight / 2 : top + ROW_HEIGHT - body.clientHeight;
    }
  }, [scrollTarget]);

  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  const last = Math.min(total, Math.ceil((scrollTop + height) / ROW_HEIGHT) + OVERSCAN);

  // Rows arrive in pages; `version` changes when one does.
  useEffect(() => {
    onNeedRows(first, last);
  }, [first, last, history.snapshot, onNeedRows]);
  const rows = [];
  for (let i = first; i < last; i++) {
    const c = commits[i];
    const top = i * ROW_HEIGHT;
    if (!c) {
      rows.push(
        <div key={`pending-${i}`} className="commit-row row-grid pending" style={{ top, height: ROW_HEIGHT }} aria-hidden="true">
          <span />
          <span />
          <span className="skeleton" />
        </div>,
      );
      continue;
    }
    rows.push(
      <div
        key={c.id}
        data-id={c.id}
        className={`commit-row row-grid${c.id === selected ? " selected" : ""}${matches?.has(i) ? " match" : ""}`}
        style={{ top, height: ROW_HEIGHT, ["--lane-color" as string]: laneColor(c.column) }}
        onClick={() => onSelect(c.id, i)}
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
      </div>,
    );
  }

  return (
    <section className={`history${matches ? " searching" : ""}`} style={{ ["--graph-width" as string]: `${graphColumnWidth(history.width)}px` }}>
      <div className="history-header row-grid">
        <span>Branch / Tag</span>
        <span>Graph</span>
        <span>Commit message</span>
        <span className="col-author">Author</span>
        <span>Date</span>
        <span className="col-sha">SHA</span>
      </div>
      {total === 0 ? (
        <div className="empty">This repository has no commits yet.</div>
      ) : (
        <div
          className="history-body"
          ref={bodyRef}
          onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
          tabIndex={-1}
        >
          <div className="rows" style={{ height: total * ROW_HEIGHT }}>
            <div className="graph-clip">
              <GraphSvg commits={commits.slice(first, last)} firstRow={first} width={history.width} selected={selected} />
            </div>
            {rows}
          </div>
        </div>
      )}
    </section>
  );
}
