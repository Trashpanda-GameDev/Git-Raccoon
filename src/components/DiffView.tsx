import { Fragment } from "react";
import type { DiffLine, FileDiff, Hunk } from "../types";
import { StatusBadge } from "./CommitDetailsPane";

export type DiffMode = "unified" | "split";

type SplitRow = { left: DiffLine | null; right: DiffLine | null };

/**
 * Pairs a hunk's lines for side-by-side display: context lines appear on
 * both sides, and each run of deletions is matched line by line with the
 * additions that follow it.
 */
export function toSplitRows(hunk: Hunk): SplitRow[] {
  const rows: SplitRow[] = [];
  let dels: DiffLine[] = [];
  let adds: DiffLine[] = [];
  let last: "left" | "right" | "both" = "both";

  const flush = () => {
    for (let i = 0; i < Math.max(dels.length, adds.length); i++) {
      rows.push({ left: dels[i] ?? null, right: adds[i] ?? null });
    }
    dels = [];
    adds = [];
  };

  for (const line of hunk.lines) {
    switch (line.kind) {
      case "delete":
        if (adds.length > 0) flush();
        dels.push(line);
        last = "left";
        break;
      case "add":
        adds.push(line);
        last = "right";
        break;
      case "noeol":
        // Belongs to whichever side the previous line was on.
        if (last === "left") dels.push(line);
        else if (last === "right") adds.push(line);
        else {
          flush();
          rows.push({ left: line, right: line });
        }
        break;
      default:
        flush();
        rows.push({ left: line, right: line });
        last = "both";
    }
  }
  flush();
  return rows;
}

const MARKERS = { context: " ", add: "+", delete: "−", noeol: "\\" } as const;

function Cell({ line, side }: { line: DiffLine | null; side: "old" | "new" }) {
  if (!line) {
    return (
      <>
        <td className="ln filler" />
        <td className="code filler" />
      </>
    );
  }
  const number = side === "old" ? line.oldLine : line.newLine;
  return (
    <>
      <td className={`ln ${line.kind}`}>{number ?? ""}</td>
      <td className={`code ${line.kind}`}>
        <span className="marker">{MARKERS[line.kind]}</span>
        {line.content}
      </td>
    </>
  );
}

function UnifiedHunk({ hunk }: { hunk: Hunk }) {
  return (
    <>
      <tr className="hunk-header">
        <td colSpan={3}>{hunk.header}</td>
      </tr>
      {hunk.lines.map((line, i) => (
        <tr key={i}>
          <td className={`ln ${line.kind}`}>{line.oldLine ?? ""}</td>
          <td className={`ln ${line.kind}`}>{line.newLine ?? ""}</td>
          <td className={`code ${line.kind}`}>
            <span className="marker">{MARKERS[line.kind]}</span>
            {line.content}
          </td>
        </tr>
      ))}
    </>
  );
}

function SplitHunk({ hunk }: { hunk: Hunk }) {
  return (
    <>
      <tr className="hunk-header">
        <td colSpan={4}>{hunk.header}</td>
      </tr>
      {toSplitRows(hunk).map((row, i) => (
        <tr key={i}>
          <Cell line={row.left} side="old" />
          <Cell line={row.right} side="new" />
        </tr>
      ))}
    </>
  );
}

type Props = {
  diff: FileDiff | null;
  file: string;
  loading: boolean;
  error: string;
  mode: DiffMode;
  onModeChange: (mode: DiffMode) => void;
  onClose: () => void;
};

export function DiffView({ diff, file, loading, error, mode, onModeChange, onClose }: Props) {
  return (
    <section className="diff-view" aria-label={`Diff of ${file}`}>
      <div className="diff-toolbar">
        {diff && <StatusBadge status={diff.status} />}
        <span className="diff-path" title={file}>
          {diff?.oldPath && <span className="muted">{diff.oldPath} → </span>}
          {file}
        </span>
        <div className="segmented" role="group" aria-label="Diff layout">
          {(["unified", "split"] as const).map((m) => (
            <button key={m} type="button" className={mode === m ? "on" : ""} aria-pressed={mode === m} onClick={() => onModeChange(m)}>
              {m === "unified" ? "Unified" : "Split"}
            </button>
          ))}
        </div>
        <button type="button" className="icon-button close" onClick={onClose} title="Back to graph (Esc)" aria-label="Close diff">
          ×
        </button>
      </div>

      <div className={`diff-body${loading ? " stale" : ""}`}>
        {error && <div className="error">{error}</div>}
        {!diff && loading && <div className="pane-empty muted">Loading diff…</div>}
        {diff?.binary && <div className="pane-empty muted">Binary file not shown.</div>}
        {diff && !diff.binary && diff.hunks.length === 0 && (
          <div className="pane-empty muted">No content changes (mode or rename only).</div>
        )}
        {diff && !diff.binary && diff.hunks.length > 0 && (
          <table className={`diff-table ${mode}`}>
            {mode === "split" ? (
              <colgroup>
                <col className="ln-col" />
                <col />
                <col className="ln-col" />
                <col />
              </colgroup>
            ) : (
              <colgroup>
                <col className="ln-col" />
                <col className="ln-col" />
                <col />
              </colgroup>
            )}
            <tbody>
              {diff.hunks.map((hunk, i) => (
                <Fragment key={i}>{mode === "split" ? <SplitHunk hunk={hunk} /> : <UnifiedHunk hunk={hunk} />}</Fragment>
              ))}
            </tbody>
          </table>
        )}
        {diff?.truncated && <div className="pane-empty muted">This diff is too large; only the first 20,000 lines are shown.</div>}
      </div>
    </section>
  );
}
