//! Word-level differences between a deleted line and the line that
//! replaced it, and pairing deleted lines with their replacements.

use std::ops::Range;

use crate::details::{DiffLine, Hunk, LineKind};

/// Skip word diffs whose token grid would exceed this (O(n·m) work).
const MAX_CELLS: usize = 200_000;
/// Lines sharing less than this fraction of their text are just "changed".
const MIN_SIMILARITY: f32 = 0.35;
/// How many added lines ahead to look for a deleted line's counterpart.
const LOOKAHEAD: usize = 8;

/// Changed character ranges (byte offsets) in the old and new line.
#[derive(Debug, Clone, PartialEq)]
pub struct WordDiff {
    pub old: Vec<Range<usize>>,
    pub new: Vec<Range<usize>>,
    /// Share of the longer line's (trimmed) text the two lines have in common.
    pub similarity: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Word,
    Space,
    Other,
}

fn class(c: char) -> Class {
    if c.is_alphanumeric() || c == '_' {
        Class::Word
    } else if c.is_whitespace() {
        Class::Space
    } else {
        Class::Other
    }
}

/// Splits a line into words, runs of whitespace and single punctuation
/// characters, as byte ranges.
pub fn tokenize(line: &str) -> Vec<Range<usize>> {
    let mut tokens: Vec<Range<usize>> = Vec::new();
    let mut prev: Option<Class> = None;
    for (i, c) in line.char_indices() {
        let k = class(c);
        match tokens.last_mut() {
            Some(last) if prev == Some(k) && k != Class::Other => last.end = i + c.len_utf8(),
            _ => tokens.push(i..i + c.len_utf8()),
        }
        prev = Some(k);
    }
    tokens
}

fn changed(tokens: &[Range<usize>], kept: &[bool]) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for (t, keep) in tokens.iter().zip(kept) {
        if *keep {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.end == t.start => last.end = t.end,
            _ => out.push(t.clone()),
        }
    }
    out
}

/// Finds the words that differ between a deleted line and the added line
/// that replaced it, via the longest common subsequence of their tokens.
/// Returns `None` when the lines are too different (or too long) for word
/// marks to help.
pub fn word_diff(old: &str, new: &str) -> Option<WordDiff> {
    let a = tokenize(old);
    let b = tokenize(new);
    if a.is_empty() || b.is_empty() || a.len() * b.len() > MAX_CELLS {
        return None;
    }
    let ta: Vec<&str> = a.iter().map(|r| &old[r.clone()]).collect();
    let tb: Vec<&str> = b.iter().map(|r| &new[r.clone()]).collect();

    // lcs[i][j] = LCS length of ta[i..] and tb[j..]
    let width = tb.len() + 1;
    let mut lcs = vec![0u32; (ta.len() + 1) * width];
    for i in (0..ta.len()).rev() {
        for j in (0..tb.len()).rev() {
            lcs[i * width + j] = if ta[i] == tb[j] {
                lcs[(i + 1) * width + j + 1] + 1
            } else {
                lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
            };
        }
    }
    let mut kept_a = vec![false; ta.len()];
    let mut kept_b = vec![false; tb.len()];
    let (mut i, mut j) = (0, 0);
    while i < ta.len() && j < tb.len() {
        if ta[i] == tb[j] {
            kept_a[i] = true;
            kept_b[j] = true;
            i += 1;
            j += 1;
        } else if lcs[(i + 1) * width + j] >= lcs[i * width + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    // Whitespace-only agreement does not make lines similar.
    let common: usize = ta
        .iter()
        .zip(&kept_a)
        .filter(|(t, k)| **k && !t.trim().is_empty())
        .map(|(t, _)| t.chars().count())
        .sum();
    let longest = old
        .trim()
        .chars()
        .count()
        .max(new.trim().chars().count())
        .max(1);
    #[expect(
        clippy::cast_precision_loss,
        reason = "line lengths are far below f32 precision limits"
    )]
    let similarity = common as f32 / longest as f32;
    if common == 0 || similarity < MIN_SIMILARITY {
        return None;
    }
    Some(WordDiff {
        old: changed(&a, &kept_a),
        new: changed(&b, &kept_b),
        similarity,
    })
}

/// For each run of deleted lines followed by added lines in a hunk, matches
/// each deleted line with the most similar added line among the next few
/// (keeping order), so inserted lines don't shift the pairing. Returns
/// `(deleted index, added index, words)` with indices into `hunk.lines`.
pub fn pair_lines(hunk: &Hunk) -> Vec<(usize, usize, WordDiff)> {
    let lines: &[DiffLine] = &hunk.lines;
    let mut pairs = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind != LineKind::Delete {
            i += 1;
            continue;
        }
        let mut dels = Vec::new();
        while i < lines.len() && lines[i].kind == LineKind::Delete {
            dels.push(i);
            i += 1;
        }
        let mut adds = Vec::new();
        while i < lines.len() && matches!(lines[i].kind, LineKind::Add | LineKind::NoEol) {
            if lines[i].kind == LineKind::Add {
                adds.push(i);
            }
            i += 1;
        }
        let mut from = 0;
        for &d in &dels {
            let best = (from..adds.len().min(from + LOOKAHEAD))
                .filter_map(|k| {
                    word_diff(&lines[d].content, &lines[adds[k]].content).map(|w| (k, w))
                })
                .max_by(|x, y| x.1.similarity.total_cmp(&y.1.similarity));
            if let Some((k, words)) = best {
                pairs.push((d, adds[k], words));
                from = k + 1;
            }
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts<'a>(line: &'a str, ranges: &[Range<usize>]) -> Vec<&'a str> {
        ranges.iter().map(|r| &line[r.clone()]).collect()
    }

    #[test]
    fn tokenizes_words_spaces_and_punctuation() {
        let line = "a_b  (c)é";
        let tokens: Vec<&str> = tokenize(line).into_iter().map(|r| &line[r]).collect();
        assert_eq!(tokens, ["a_b", "  ", "(", "c", ")", "é"]);
        assert_eq!(texts("c)é", &tokenize("c)é")), ["c", ")", "é"]);
    }

    #[test]
    fn marks_only_added_words() {
        let new = "let total = sum(a, b, c);";
        let d = word_diff("let total = sum(a, b);", new).unwrap();
        assert!(d.old.is_empty());
        assert_eq!(texts(new, &d.new), [", c"]);
    }

    #[test]
    fn marks_replaced_words_on_both_sides() {
        let (old, new) = ("return Err(Error);", "return Err(LoadError::Bare);");
        let d = word_diff(old, new).unwrap();
        assert_eq!(texts(old, &d.old), ["Error"]);
        assert_eq!(texts(new, &d.new), ["LoadError::Bare"]);
    }

    #[test]
    fn gives_up_on_unrelated_lines() {
        assert!(word_diff("fn main() {}", "# Heading text here").is_none());
        assert!(word_diff("", "x").is_none());
    }

    fn line(kind: LineKind, content: &str) -> DiffLine {
        DiffLine {
            kind,
            old_line: None,
            new_line: None,
            content: content.into(),
        }
    }

    #[test]
    fn pairs_by_similarity_not_position() {
        let hunk = Hunk {
            header: "@@".into(),
            lines: vec![
                line(
                    LineKind::Delete,
                    "pub fn open(path: &Path) -> Result<Repo, Error> {",
                ),
                line(LineKind::Delete, "    let repo = Repo::discover(path)?;"),
                line(LineKind::Add, "/// Opens the repository containing `path`."),
                line(
                    LineKind::Add,
                    "pub fn open(path: &Path) -> Result<Repo, LoadError> {",
                ),
                line(
                    LineKind::Add,
                    "    let repo = Repo::discover(path).map_err(LoadError::NotARepo)?;",
                ),
                line(LineKind::Add, "    if repo.is_bare() {"),
            ],
        };
        let pairs = pair_lines(&hunk);
        let summary: Vec<(usize, usize)> = pairs.iter().map(|(d, a, _)| (*d, *a)).collect();
        assert_eq!(summary, [(0, 3), (1, 4)]);
        assert_eq!(texts(&hunk.lines[0].content, &pairs[0].2.old), ["Error"]);
        assert_eq!(
            texts(&hunk.lines[4].content, &pairs[1].2.new).concat(),
            ".map_err(LoadError::NotARepo)"
        );
    }
}
