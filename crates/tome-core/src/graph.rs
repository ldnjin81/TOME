//! Lane layout for the revision graph (the Smartlog's full view).
//!
//! Lore's history call follows one branch's first-parent chain, so a merge's second parent lives
//! in another branch's history. The caller loads every branch, joins the lists and passes them
//! here. The layout is the usual one of `git log --graph`:
//!
//! - Rows go children first (a topological order); among revisions that are ready, the newest
//!   timestamp goes first, so a clock that runs behind never puts a parent above its child.
//! - Each lane waits for one revision. A revision takes the leftmost lane waiting for it (or the
//!   first free lane when nothing waits: a branch head); other lanes waiting for it join it there.
//! - Its first parent continues in its own lane; another parent goes to the lane already waiting
//!   for it, or to a new lane.
//! - Lanes never move sideways, and a freed lane is reused by the next new one.
//! - A parent that is not in the list (history cut short, or a merged branch that was not
//!   loaded) gets no lane: the row lists it in `missing` and the line ends there, so unloaded
//!   history never leaves lanes running to the bottom.
//!
//! [`ascii`] draws the rows as text, which is what the tests compare.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use serde::Serialize;

use crate::model::Revision;

/// Where a segment starts or ends in its row: the top edge, the node's height, the bottom edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum End {
    Top,
    Mid,
    Bottom,
}

/// A line inside one row, from (`from_lane`, `from`) to (`to_lane`, `to`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Segment {
    pub from_lane: usize,
    pub from: End,
    pub to_lane: usize,
    pub to: End,
    /// The revision this line leads down to (its color comes from that revision's branch).
    pub target: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub revision: Revision,
    /// The node's lane.
    pub lane: usize,
    /// Lanes this row draws in (the widest of its top and bottom edges).
    pub width: usize,
    pub segments: Vec<Segment>,
    /// Parents not in the list, in parent order (the first one is the first parent).
    pub missing: Vec<String>,
}

/// Heap entry: newest first, then the higher revision number, then the earlier input.
#[derive(PartialEq, Eq)]
struct Ready {
    timestamp: u64,
    number: u64,
    index: std::cmp::Reverse<usize>,
}

impl Ord for Ready {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.timestamp, self.number, &self.index).cmp(&(other.timestamp, other.number, &other.index))
    }
}

impl PartialOrd for Ready {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Children-first order; duplicates (the same revision from two branches) are dropped.
fn order(revisions: &[Revision]) -> Vec<&Revision> {
    let mut seen = HashSet::new();
    let unique: Vec<&Revision> = revisions.iter().filter(|r| seen.insert(r.id.as_str())).collect();
    let index: HashMap<&str, usize> = unique.iter().enumerate().map(|(i, r)| (r.id.as_str(), i)).collect();
    let mut children = vec![0usize; unique.len()];
    for r in &unique {
        for p in &r.parents {
            if let Some(&i) = index.get(p.as_str()) {
                children[i] += 1;
            }
        }
    }
    let ready = |i: usize| Ready { timestamp: unique[i].timestamp, number: unique[i].number, index: std::cmp::Reverse(i) };
    let mut heap: BinaryHeap<Ready> = (0..unique.len()).filter(|&i| children[i] == 0).map(ready).collect();
    let mut out = Vec::with_capacity(unique.len());
    while let Some(Ready { index: std::cmp::Reverse(i), .. }) = heap.pop() {
        out.push(unique[i]);
        for p in &unique[i].parents {
            if let Some(&j) = index.get(p.as_str()) {
                children[j] -= 1;
                if children[j] == 0 {
                    heap.push(ready(j));
                }
            }
        }
    }
    out
}

fn free_lane(lanes: &mut Vec<Option<String>>, except: usize) -> usize {
    match lanes.iter().enumerate().position(|(i, l)| l.is_none() && i != except) {
        Some(i) => i,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

/// Lays out `revisions` (any order, duplicates allowed). Parents that are not in the list keep
/// their lane open to the bottom.
pub fn layout(revisions: &[Revision]) -> Vec<Row> {
    let present: HashSet<&str> = revisions.iter().map(|r| r.id.as_str()).collect();
    let mut lanes: Vec<Option<String>> = Vec::new();
    let mut rows = Vec::new();
    for revision in order(revisions) {
        let id = revision.id.as_str();
        let waiting: Vec<usize> = (0..lanes.len()).filter(|&i| lanes[i].as_deref() == Some(id)).collect();
        let lane = match waiting.first() {
            Some(&lane) => lane,
            None => free_lane(&mut lanes, usize::MAX),
        };
        let top_width = lanes.len();
        let mut segments = Vec::new();
        for (i, expected) in lanes.iter().enumerate() {
            match expected {
                Some(e) if e == id => segments.push(Segment { from_lane: i, from: End::Top, to_lane: lane, to: End::Mid, target: id.to_string() }),
                Some(e) => segments.push(Segment { from_lane: i, from: End::Top, to_lane: i, to: End::Bottom, target: e.clone() }),
                None => {}
            }
        }
        for &i in &waiting {
            lanes[i] = None;
        }

        let missing: Vec<String> = revision.parents.iter().filter(|p| !present.contains(p.as_str())).cloned().collect();
        let mut parents = revision.parents.iter();
        if let Some(first) = parents.next().filter(|p| present.contains(p.as_str())) {
            lanes[lane] = Some(first.clone());
            segments.push(Segment { from_lane: lane, from: End::Mid, to_lane: lane, to: End::Bottom, target: first.clone() });
        }
        for parent in parents.filter(|p| present.contains(p.as_str())) {
            let to = match lanes.iter().position(|l| l.as_deref() == Some(parent.as_str())) {
                Some(existing) => existing,
                None => {
                    let k = free_lane(&mut lanes, lane);
                    lanes[k] = Some(parent.clone());
                    k
                }
            };
            segments.push(Segment { from_lane: lane, from: End::Mid, to_lane: to, to: End::Bottom, target: parent.clone() });
        }
        while lanes.last().is_some_and(Option::is_none) {
            lanes.pop();
        }
        let width = top_width.max(lanes.len()).max(lane + 1);
        rows.push(Row { revision: revision.clone(), lane, width, segments, missing });
    }
    rows
}

/// Draws the rows like `git log --graph`: `*` a revision, `|` a lane going straight, `\` and `/`
/// a line moving to another lane between two rows (`-` stretches it over more than one lane).
/// Each node line ends with `label(revision)`, then `(+N not loaded)` for parents not in the list.
pub fn ascii(rows: &[Row], label: impl Fn(&Revision) -> String) -> String {
    let width = rows.iter().map(|r| r.width).max().unwrap_or(0);
    let pad = (width * 2).saturating_sub(1);
    let mut out = String::new();
    for (n, row) in rows.iter().enumerate() {
        let mut line = vec![' '; pad];
        line[row.lane * 2] = '*';
        for s in &row.segments {
            if s.from == End::Top && s.to == End::Bottom {
                line[s.from_lane * 2] = '|';
            }
        }
        let text: String = line.into_iter().collect();
        let missing = if row.missing.is_empty() { String::new() } else { format!(" (+{} not loaded)", row.missing.len()) };
        out.push_str(&format!("{text}  {}{missing}\n", label(&row.revision)));

        // Between this row and the next: this row's lines out of the node and straight on, and
        // the next row's lines into its node.
        let Some(next) = rows.get(n + 1) else { break };
        // Slashes first, then lanes going straight, then `-` only where nothing else is drawn,
        // so two lines moving in the same gap both stay visible.
        let mut between = vec![' '; pad];
        let mut straight = HashSet::new();
        let mut slashes = Vec::new();
        let mut fills = Vec::new();
        for s in &row.segments {
            if s.to != End::Bottom {
                continue;
            }
            if s.from_lane == s.to_lane {
                straight.insert(s.to_lane);
            } else if s.to_lane > s.from_lane {
                slashes.push((s.to_lane * 2 - 1, '\\'));
                fills.extend(s.from_lane * 2 + 1..s.to_lane * 2 - 1);
            } else {
                slashes.push((s.to_lane * 2 + 1, '/'));
                fills.extend(s.to_lane * 2 + 2..s.from_lane * 2);
            }
        }
        for s in &next.segments {
            if s.from == End::Top && s.to == End::Mid && s.from_lane != s.to_lane {
                straight.remove(&s.from_lane);
                slashes.push((s.from_lane * 2 - 1, '/'));
                fills.extend(s.to_lane * 2 + 1..s.from_lane * 2 - 1);
            }
        }
        let slanted = !slashes.is_empty();
        for (c, ch) in slashes {
            between[c] = ch;
        }
        for lane in straight {
            between[lane * 2] = '|';
        }
        for c in fills {
            if between[c] == ' ' {
                between[c] = '-';
            }
        }
        if slanted {
            out.push_str(between.into_iter().collect::<String>().trim_end());
            out.push('\n');
        }
    }
    out.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `rev("M2", &["M1", "F2"], "main", 5)`: a revision on `branch` at time `t`.
    fn rev(id: &str, parents: &[&str], branch: &str, t: u64) -> Revision {
        Revision {
            id: id.into(),
            number: t,
            parents: parents.iter().map(|p| p.to_string()).collect(),
            message: String::new(),
            author: String::new(),
            timestamp: t,
            branch_id: branch.into(),
            metadata: Vec::new(),
        }
    }

    fn draw(revisions: &[Revision]) -> String {
        ascii(&layout(revisions), |r| format!("{} ({})", r.id, r.branch_id))
    }

    fn check(revisions: &[Revision], expected: &str) {
        let got = draw(revisions);
        let expected = expected.trim_matches('\n').lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
        assert_eq!(got, expected, "\n--- got ---\n{got}\n--- expected ---\n{expected}\n");
    }

    #[test]
    fn empty() {
        assert!(layout(&[]).is_empty());
        assert_eq!(draw(&[]), "");
    }

    #[test]
    fn linear() {
        check(
            &[rev("C3", &["C2"], "main", 3), rev("C2", &["C1"], "main", 2), rev("C1", &[], "main", 1)],
            "
*  C3 (main)
*  C2 (main)
*  C1 (main)",
        );
    }

    #[test]
    fn feature_branch_merged() {
        // main: B - M1 - M2(merge); feature: B - F1 - F2, merged into M2.
        check(
            &[
                rev("M2", &["M1", "F2"], "main", 5),
                rev("F2", &["F1"], "feature", 4),
                rev("M1", &["B"], "main", 3),
                rev("F1", &["B"], "feature", 2),
                rev("B", &[], "main", 1),
            ],
            r"
*    M2 (main)
|\
| *  F2 (feature)
* |  M1 (main)
| *  F1 (feature)
|/
*    B (main)",
        );
    }

    #[test]
    fn open_branch_not_merged() {
        // The feature head is newer than main's, so it comes first in its own lane.
        check(
            &[rev("M1", &["B"], "main", 2), rev("F1", &["B"], "feature", 3), rev("B", &[], "main", 1)],
            r"
*    F1 (feature)
| *  M1 (main)
|/
*    B (main)",
        );
    }

    #[test]
    fn two_branches_merged_one_after_another() {
        check(
            &[
                rev("M2", &["M1", "C1"], "main", 5),
                rev("M1", &["B", "A1"], "main", 4),
                rev("C1", &["B"], "c", 3),
                rev("A1", &["B"], "a", 2),
                rev("B", &[], "main", 1),
            ],
            r"
*      M2 (main)
|\
* |    M1 (main)
|-|\
| * |  C1 (c)
| | *  A1 (a)
|/-/
*      B (main)",
        );
    }

    #[test]
    fn main_merged_into_branch_then_back() {
        // feature takes main's M1 (F2), then main takes the feature (M2).
        check(
            &[
                rev("M2", &["M1", "F2"], "main", 5),
                rev("F2", &["F1", "M1"], "feature", 4),
                rev("M1", &["B"], "main", 3),
                rev("F1", &["B"], "feature", 2),
                rev("B", &[], "main", 1),
            ],
            r"
*    M2 (main)
|\
| *  F2 (feature)
|/|
* |  M1 (main)
| *  F1 (feature)
|/
*    B (main)",
        );
    }

    #[test]
    fn branch_from_a_branch() {
        // g branches off f, merges into f, and f merges into main.
        check(
            &[
                rev("M1", &["B", "F2"], "main", 5),
                rev("F2", &["F1", "G1"], "f", 4),
                rev("G1", &["F1"], "g", 3),
                rev("F1", &["B"], "f", 2),
                rev("B", &[], "main", 1),
            ],
            r"
*      M1 (main)
|\
| *    F2 (f)
| |\
| | *  G1 (g)
| |/
| *    F1 (f)
|/
*      B (main)",
        );
    }

    #[test]
    fn child_older_than_parent_still_comes_first() {
        // A clock that runs behind: C2's timestamp is before its parent's.
        check(
            &[rev("C1", &[], "main", 10), rev("C2", &["C1"], "main", 5)],
            "
*  C2 (main)
*  C1 (main)",
        );
    }

    #[test]
    fn missing_parents_end_without_a_lane() {
        // History cut at 2 revisions: the merge's second parent and M2's parent are not loaded.
        let revisions = [rev("M3", &["M2", "X9"], "main", 3), rev("M2", &["M1"], "main", 2)];
        let rows = layout(&revisions);
        assert_eq!(rows[0].missing, ["X9"]);
        assert_eq!(rows[1].missing, ["M1"]);
        // No lane is kept for X9, so M2's row is one lane wide.
        assert_eq!(rows[1].width, 1);
        check(
            &revisions,
            "
*  M3 (main) (+1 not loaded)
*  M2 (main) (+1 not loaded)",
        );
    }

    #[test]
    fn many_merges_of_unloaded_branches_stay_one_lane() {
        // Like SampleProject read offline: every merge's branch side is missing locally.
        let revisions = [
            rev("M4", &["M3", "A"], "main", 4),
            rev("M3", &["M2", "B"], "main", 3),
            rev("M2", &["M1", "C"], "main", 2),
            rev("M1", &[], "main", 1),
        ];
        assert!(layout(&revisions).iter().all(|r| r.width == 1));
        check(
            &revisions,
            "
*  M4 (main) (+1 not loaded)
*  M3 (main) (+1 not loaded)
*  M2 (main) (+1 not loaded)
*  M1 (main)",
        );
    }

    #[test]
    fn duplicates_from_two_branch_histories() {
        let base = rev("B", &[], "main", 1);
        let f1 = rev("F1", &["B"], "f", 2);
        check(&[f1.clone(), base.clone(), base, f1], "\n*  F1 (f)\n*  B (main)");
    }

    #[test]
    fn merge_of_a_branch_with_no_new_revisions() {
        // The second parent is an older main revision: the side lane joins main again at B.
        check(
            &[rev("M2", &["M1", "B"], "main", 3), rev("M1", &["B"], "main", 2), rev("B", &[], "main", 1)],
            r"
*    M2 (main)
|\
* |  M1 (main)
|/
*    B (main)",
        );
    }

    #[test]
    fn freed_lane_is_reused() {
        // a merges first; the lane it used is free again when b starts below.
        check(
            &[
                rev("M2", &["M1", "A1"], "main", 6),
                rev("A1", &["M1"], "a", 5),
                rev("M1", &["B", "B1"], "main", 4),
                rev("B1", &["B"], "b", 3),
                rev("B", &[], "main", 1),
            ],
            r"
*    M2 (main)
|\
| *  A1 (a)
|/
*    M1 (main)
|\
| *  B1 (b)
|/
*    B (main)",
        );
    }

    #[test]
    fn two_roots() {
        // Two unrelated histories (e.g. a repository joined from two): each keeps its lane.
        check(
            &[rev("M1", &["A", "X"], "main", 3), rev("X", &[], "other", 2), rev("A", &[], "main", 1)],
            r"
*    M1 (main)
|\
| *  X (other)
*    A (main)",
        );
    }
}
