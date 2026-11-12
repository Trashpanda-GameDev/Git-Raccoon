import { memo } from "react";
import type { GraphCommit, RefLabel } from "../types";

export const ROW_HEIGHT = 28;
const LANE_WIDTH = 18;
const GRAPH_PADDING = 12;
const DOT_RADIUS = 6;

const LANE_COLORS = [
  "#15a0bf", "#0669f7", "#8e00c2", "#c517b6",
  "#d90171", "#cd0101", "#f25d2e", "#f2ca33",
  "#7bd938", "#2ece9d",
];

export const laneColor = (lane: number) => LANE_COLORS[lane % LANE_COLORS.length];

const laneX = (lane: number) => GRAPH_PADDING + lane * LANE_WIDTH;
const rowY = (row: number) => row * ROW_HEIGHT + ROW_HEIGHT / 2;

// At least wide enough for the "Graph" column heading.
export const graphWidth = (lanes: number) => Math.max(Math.max(lanes, 1) * LANE_WIDTH + GRAPH_PADDING, 64);

function edgePath(row: number, from: number, to: number) {
  const x1 = laneX(from);
  const x2 = laneX(to);
  const y1 = rowY(row);
  const y2 = rowY(row + 1);
  if (x1 === x2) return `M${x1} ${y1}V${y2}`;
  // An S-curve between the two lanes, vertical at both ends.
  const mid = (y1 + y2) / 2;
  return `M${x1} ${y1}C${x1} ${mid} ${x2} ${mid} ${x2} ${y2}`;
}

type GraphProps = {
  commits: GraphCommit[];
  width: number;
  selected: string | null;
};

/** The lanes, edges and dots for every row, drawn as one SVG behind the rows. */
export const GraphSvg = memo(function GraphSvg({ commits, width, selected }: GraphProps) {
  const height = commits.length * ROW_HEIGHT;
  return (
    <svg className="graph-svg" width={graphWidth(width)} height={height} aria-hidden="true">
      <g fill="none" strokeWidth={2}>
        {commits.map((c, row) =>
          c.edges.map((e, i) => (
            <path key={`${row}-${i}`} d={edgePath(row, e.from, e.to)} stroke={laneColor(e.lane)} />
          )),
        )}
      </g>
      {commits.map((c, row) => {
        const color = laneColor(c.column);
        const isMerge = c.parents.length > 1;
        const isHead = c.refs.some((r) => r.isHead);
        return (
          <g key={c.id}>
            {(isHead || c.id === selected) && (
              <circle cx={laneX(c.column)} cy={rowY(row)} r={DOT_RADIUS + 3} fill="none" stroke={color} strokeWidth={2} />
            )}
            <circle
              cx={laneX(c.column)}
              cy={rowY(row)}
              r={isMerge ? DOT_RADIUS - 2 : DOT_RADIUS}
              fill={isMerge ? color : "var(--bg)"}
              stroke={color}
              strokeWidth={isMerge ? 0 : 3}
            />
          </g>
        );
      })}
    </svg>
  );
});

const REF_ICONS: Record<RefLabel["kind"], string> = {
  head: "◆",
  local: "",
  remote: "☁",
  tag: "🏷",
};

export function RefPills({ refs, lane }: { refs: RefLabel[]; lane: number }) {
  if (refs.length === 0) return null;
  const color = laneColor(lane);
  const [first, ...rest] = refs;
  return (
    <div className="ref-pills">
      <span
        className={`ref-pill ref-${first.kind}${first.isHead ? " ref-current" : ""}`}
        style={{ borderColor: color, background: first.isHead ? color : undefined }}
        title={refs.map((r) => r.name).join("\n")}
      >
        {first.isHead && first.kind === "local" && <span className="ref-check">✓</span>}
        {REF_ICONS[first.kind] && <span className="ref-icon">{REF_ICONS[first.kind]}</span>}
        <span className="ref-name">{first.name}</span>
      </span>
      {rest.length > 0 && (
        <span className="ref-more" style={{ borderColor: color }} title={rest.map((r) => r.name).join("\n")}>
          +{rest.length}
        </span>
      )}
    </div>
  );
}
