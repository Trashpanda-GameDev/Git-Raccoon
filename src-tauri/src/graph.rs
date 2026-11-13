//! Lane layout for the commit graph.
//!
//! Commits arrive newest first in topological order. Each row gets a column
//! (lane) for its dot, and a list of edges that run from this row down to the
//! next one. A lane "expects" the commit that will continue it; a commit takes
//! the first lane expecting it, and any other lanes expecting it merge in.

use serde::Serialize;

/// A line drawn from `from` (a column at this row) to `to` (a column at the
/// next row). `lane` picks the colour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub lane: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowLayout {
    pub column: usize,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

/// Lays out `(id, parent ids)` pairs, given newest first in topological order.
pub fn layout<S: AsRef<str>>(commits: &[(S, Vec<S>)]) -> Layout {
    let mut lanes: Vec<Option<&str>> = Vec::new();
    let mut rows: Vec<RowLayout> = Vec::new();
    // Edges leaving the previous row, as (from column, lane); their end column
    // is only known once we place the next commit.
    let mut pending: Vec<(usize, usize)> = Vec::new();
    let mut width = 0;

    for (id, parents) in commits {
        let id = id.as_ref();
        let column = match lanes.iter().position(|l| *l == Some(id)) {
            Some(i) => i,
            None => free_slot(&mut lanes),
        };

        if let Some(prev) = rows.last_mut() {
            prev.edges = pending
                .drain(..)
                .map(|(from, lane)| Edge {
                    from,
                    to: if lanes[lane] == Some(id) {
                        column
                    } else {
                        lane
                    },
                    lane,
                })
                .collect();
        }

        // Lanes that were waiting for this commit end here.
        for lane in lanes.iter_mut() {
            if *lane == Some(id) {
                *lane = None;
            }
        }
        // Everything still open passes straight through this row.
        for (k, lane) in lanes.iter().enumerate() {
            if lane.is_some() {
                pending.push((k, k));
            }
        }

        if let Some((first, rest)) = parents.split_first() {
            // The first parent continues in this commit's lane.
            lanes[column] = Some(first.as_ref());
            pending.push((column, column));
            // Merge parents join a lane already waiting for them, or open one.
            for parent in rest {
                let parent = parent.as_ref();
                let lane = match lanes.iter().position(|l| *l == Some(parent)) {
                    Some(m) => m,
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
            edges: Vec::new(),
        });
    }

    // Edges out of the last row lead to commits beyond the limit: draw them
    // straight down.
    if let Some(last) = rows.last_mut() {
        last.edges = pending
            .drain(..)
            .map(|(from, lane)| Edge {
                from,
                to: lane,
                lane,
            })
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

    fn e(from: usize, to: usize, lane: usize) -> Edge {
        Edge { from, to, lane }
    }

    #[test]
    fn linear_history_stays_in_one_lane() {
        let l = layout(&[c("c", &["b"]), c("b", &["a"]), c("a", &[])]);
        assert_eq!(l.width, 1);
        assert_eq!(
            l.rows.iter().map(|r| r.column).collect::<Vec<_>>(),
            [0, 0, 0]
        );
        assert_eq!(l.rows[0].edges, [e(0, 0, 0)]);
        assert_eq!(l.rows[1].edges, [e(0, 0, 0)]);
        assert!(l.rows[2].edges.is_empty());
    }

    #[test]
    fn merge_opens_a_lane_and_closes_it_at_the_fork_point() {
        // m merges f into main; f and b both come from a.
        let l = layout(&[
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
        assert_eq!(l.rows[0].edges, [e(0, 0, 0), e(0, 1, 1)]);
        // At f: lane 0 passes through, f continues down lane 1.
        assert_eq!(l.rows[1].edges, [e(0, 0, 0), e(1, 1, 1)]);
        // At b: lane 1 (waiting for a) passes, b continues to a; both meet at a.
        assert_eq!(l.rows[2].edges, [e(1, 0, 1), e(0, 0, 0)]);
    }

    #[test]
    fn separate_branch_tips_get_their_own_lanes() {
        let l = layout(&[c("x", &["a"]), c("y", &["a"]), c("a", &[])]);
        assert_eq!(
            l.rows.iter().map(|r| r.column).collect::<Vec<_>>(),
            [0, 1, 0]
        );
        assert_eq!(l.rows[0].edges, [e(0, 0, 0)]);
        assert_eq!(l.rows[1].edges, [e(0, 0, 0), e(1, 0, 1)]);
    }

    #[test]
    fn freed_lanes_are_reused() {
        // After the merge closes lane 1, the next branch tip reuses it
        // instead of widening the graph.
        let l = layout(&[
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
        let l = layout(&[c("b", &["a"])]);
        assert_eq!(l.rows[0].edges, [e(0, 0, 0)]);
    }
}
