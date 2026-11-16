//! Lane layout for the commit graph.
//!
//! Commits arrive newest first in topological order. Each row gets a column
//! (lane) for its dot, and a list of edges that run from this row down to the
//! next one. A lane "expects" the commit that will continue it; a commit takes
//! the first lane expecting it, and any other lanes expecting it merge in.
//!
//! The checked-out commit, if given, keeps the leftmost lane, and so does
//! the line of its first parents below it.

/// A line drawn from `from` (a column at this row) to `to` (a column at the
/// next row). `lane` picks the colour. Columns are `u16` because the layout
/// stores one edge per open lane per row, which adds up on big histories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: u16,
    pub to: u16,
    pub lane: u16,
}

impl Edge {
    fn new(from: usize, to: usize, lane: usize) -> Self {
        // More than 65,535 simultaneous lanes would draw wrongly, not crash.
        let narrow = |x: usize| u16::try_from(x).unwrap_or(u16::MAX);
        Edge {
            from: narrow(from),
            to: narrow(to),
            lane: narrow(lane),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowLayout {
    pub column: usize,
    pub edges: Box<[Edge]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub rows: Vec<RowLayout>,
    /// Number of columns the graph needs.
    pub width: usize,
}

fn free_slot<T>(lanes: &mut Vec<Option<T>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(i) => i,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

/// Lays out `(id, parent ids)` pairs, given newest first in topological
/// order, with `main` (the checked-out commit) in the leftmost lane.
pub fn layout<T: PartialEq>(commits: &[(T, Vec<T>)], main: Option<&T>) -> Layout {
    let mut lanes: Vec<Option<&T>> = Vec::new();
    // The leftmost lane is kept for `main` from the top, though nothing is
    // drawn in it until a line reaches it.
    let main = main.filter(|m| commits.iter().any(|(id, _)| id == *m));
    let mut idle = None;
    if let Some(m) = main {
        lanes.push(Some(m));
        idle = Some(0);
    }
    let mut rows: Vec<RowLayout> = Vec::new();
    // Edges leaving the previous row, as (from column, lane); their end column
    // is only known once we place the next commit.
    let mut pending: Vec<(usize, usize)> = Vec::new();
    let mut width = 0;

    for (id, parents) in commits {
        let column = match lanes.iter().position(|l| *l == Some(id)) {
            Some(i) => i,
            None => free_slot(&mut lanes),
        };

        if let Some(prev) = rows.last_mut() {
            prev.edges = pending
                .drain(..)
                .map(|(from, lane)| {
                    let to = if lanes[lane] == Some(id) {
                        column
                    } else {
                        lane
                    };
                    Edge::new(from, to, lane)
                })
                .collect();
        }

        if idle == Some(column) {
            idle = None;
        }
        // Lanes that were waiting for this commit end here.
        for lane in lanes.iter_mut() {
            if *lane == Some(id) {
                *lane = None;
            }
        }
        // Everything still open passes straight through this row.
        for (k, lane) in lanes.iter().enumerate() {
            if lane.is_some() && idle != Some(k) {
                pending.push((k, k));
            }
        }

        if let Some((first, rest)) = parents.split_first() {
            // The first parent continues in this commit's lane.
            lanes[column] = Some(first);
            pending.push((column, column));
            // Merge parents join a lane already waiting for them, or open one.
            for parent in rest {
                let lane = match lanes.iter().position(|l| *l == Some(parent)) {
                    Some(m) => {
                        // A line now runs down the kept lane.
                        if idle == Some(m) {
                            idle = None;
                        }
                        m
                    }
                    None => {
                        let s = free_slot(&mut lanes);
                        lanes[s] = Some(parent);
                        s
                    }
                };
                pending.push((column, lane));
            }
        }

        width = width.max(lanes.len()).max(column + 1);
        while lanes.last() == Some(&None) {
            lanes.pop();
        }
        rows.push(RowLayout {
            column,
            edges: Box::default(),
        });
    }

    // Edges out of the last row lead to commits beyond the limit: draw them
    // straight down.
    if let Some(last) = rows.last_mut() {
        last.edges = pending
            .drain(..)
            .map(|(from, lane)| Edge::new(from, lane, lane))
            .collect();
    }

    Layout { rows, width }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: &'static str, parents: &[&'static str]) -> (&'static str, Vec<&'static str>) {
        (id, parents.to_vec())
    }

    fn e(from: u16, to: u16, lane: u16) -> Edge {
        Edge { from, to, lane }
    }

    fn layout_of(commits: &[(&'static str, Vec<&'static str>)]) -> Layout {
        layout(commits, None)
    }

    fn columns(l: &Layout) -> Vec<usize> {
        l.rows.iter().map(|r| r.column).collect()
    }

    #[test]
    fn the_checked_out_commit_keeps_the_leftmost_lane() {
        // A newer branch tip x, then the checked-out h; both from a.
        let commits = [c("x", &["a"]), c("h", &["a"]), c("a", &["z"]), c("z", &[])];
        assert_eq!(columns(&layout_of(&commits)), [0, 1, 0, 0], "without");
        let l = layout(&commits, Some(&"h"));
        assert_eq!(columns(&l), [1, 0, 0, 0]);
        // Nothing is drawn in the kept lane above h.
        assert_eq!(*l.rows[0].edges, [e(1, 1, 1)]);
        // x's line comes in to a; h's goes straight down.
        assert_eq!(*l.rows[1].edges, [e(1, 0, 1), e(0, 0, 0)]);
        // Not in the history (beyond the limit): as before.
        assert_eq!(columns(&layout(&commits, Some(&"q"))), [0, 1, 0, 0]);
    }

    #[test]
    fn a_line_into_the_kept_lane_is_drawn_down_to_it() {
        // m (newest) merges h, the checked-out commit, into x.
        let l = layout(
            &[
                c("m", &["x", "h"]),
                c("x", &["a"]),
                c("h", &["a"]),
                c("a", &[]),
            ],
            Some(&"h"),
        );
        assert_eq!(columns(&l), [1, 1, 0, 0]);
        assert_eq!(*l.rows[0].edges, [e(1, 1, 1), e(1, 0, 0)]);
        assert_eq!(*l.rows[1].edges, [e(0, 0, 0), e(1, 1, 1)]);
    }

    #[test]
    fn linear_history_stays_in_one_lane() {
        let l = layout_of(&[c("c", &["b"]), c("b", &["a"]), c("a", &[])]);
        assert_eq!(l.width, 1);
        assert_eq!(
            l.rows.iter().map(|r| r.column).collect::<Vec<_>>(),
            [0, 0, 0]
        );
        assert_eq!(*l.rows[0].edges, [e(0, 0, 0)]);
        assert_eq!(*l.rows[1].edges, [e(0, 0, 0)]);
        assert!(l.rows[2].edges.is_empty());
    }

    #[test]
    fn merge_opens_a_lane_and_closes_it_at_the_fork_point() {
        // m merges f into main; f and b both come from a.
        let l = layout_of(&[
            c("m", &["b", "f"]),
            c("f", &["a"]),
            c("b", &["a"]),
            c("a", &[]),
        ]);
        assert_eq!(l.width, 2);
        assert_eq!(
            l.rows.iter().map(|r| r.column).collect::<Vec<_>>(),
            [0, 1, 0, 0]
        );
        // m -> b continues in lane 0, m -> f bends out to lane 1.
        assert_eq!(*l.rows[0].edges, [e(0, 0, 0), e(0, 1, 1)]);
        // At f: lane 0 passes through, f continues down lane 1.
        assert_eq!(*l.rows[1].edges, [e(0, 0, 0), e(1, 1, 1)]);
        // At b: lane 1 (waiting for a) passes, b continues to a; both meet at a.
        assert_eq!(*l.rows[2].edges, [e(1, 0, 1), e(0, 0, 0)]);
    }

    #[test]
    fn separate_branch_tips_get_their_own_lanes() {
        let l = layout_of(&[c("x", &["a"]), c("y", &["a"]), c("a", &[])]);
        assert_eq!(
            l.rows.iter().map(|r| r.column).collect::<Vec<_>>(),
            [0, 1, 0]
        );
        assert_eq!(*l.rows[0].edges, [e(0, 0, 0)]);
        assert_eq!(*l.rows[1].edges, [e(0, 0, 0), e(1, 0, 1)]);
    }

    #[test]
    fn freed_lanes_are_reused() {
        // After the merge closes lane 1, the next branch tip reuses it
        // instead of widening the graph.
        let l = layout_of(&[
            c("m", &["b", "f"]),
            c("f", &["a"]),
            c("b", &["a"]),
            c("a", &["z"]),
            c("t", &["z"]),
            c("z", &[]),
        ]);
        assert_eq!(l.width, 2);
        assert_eq!(l.rows[4].column, 1);
    }

    #[test]
    fn edges_past_the_limit_go_straight_down() {
        let l = layout_of(&[c("b", &["a"])]);
        assert_eq!(*l.rows[0].edges, [e(0, 0, 0)]);
    }
}
