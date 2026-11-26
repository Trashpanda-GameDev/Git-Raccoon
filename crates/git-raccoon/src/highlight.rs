//! Turning a diff into syntax-highlighted, word-marked text ready to draw.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Mutex, OnceLock, PoisonError};

use eframe::egui::Color32;
use eframe::egui::text::{LayoutJob, TextFormat};
use git_raccoon_core::details::{DiffLine, FileDiff, Hunk, LineKind};
use git_raccoon_core::words;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Theme};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::theme;

/// Skip syntax highlighting for diffs with more text than this.
const MAX_HIGHLIGHT_BYTES: usize = 400_000;

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

/// The syntax colours for the current look.
fn syntax_theme() -> &'static Theme {
    static SET: OnceLock<two_face::theme::EmbeddedLazyThemeSet> = OnceLock::new();
    SET.get_or_init(two_face::theme::extra).get(theme::syntax())
}

/// The grammar for a file path, by extension or file name; `None` for plain text.
pub fn syntax_for(path: &str) -> Option<&'static SyntaxReference> {
    let set = syntaxes();
    let name = path.rsplit('/').next().unwrap_or(path);
    let by_ext = name
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .and_then(|(_, ext)| {
            set.find_syntax_by_extension(ext)
                .or_else(|| set.find_syntax_by_extension(&ext.to_lowercase()))
        });
    by_ext
        .or_else(|| set.find_syntax_by_extension(name))
        .filter(|s| s.name != "Plain Text")
}

/// Fenced code in Markdown, highlighted for its language (`rust`, `yaml`,
/// `sh`…), in the diffs' colours; `None` for a language without a grammar.
pub fn code(lang: &str, text: &str) -> Option<LayoutJob> {
    // Drawn every frame: the last few are kept.
    type Seen = Vec<(
        (String, String, two_face::theme::EmbeddedThemeName),
        LayoutJob,
    )>;
    static SEEN: Mutex<Seen> = Mutex::new(Vec::new());
    let key = (lang.to_string(), text.to_string(), theme::syntax());
    let mut seen = SEEN.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((_, job)) = seen.iter().find(|(k, _)| *k == key) {
        return Some(job.clone());
    }

    let token = lang.split([' ', ',', '{']).next()?.trim();
    let set = syntaxes();
    let syntax = set
        .find_syntax_by_token(token)
        .or_else(|| set.find_syntax_by_token(&token.to_lowercase()))
        .filter(|s| s.name != "Plain Text")?;
    let mut highlighter = HighlightLines::new(syntax, syntax_theme());
    let mut job = LayoutJob::default();
    let format = |color, italics| TextFormat {
        font_id: theme::mono(theme::MONO_SIZE),
        color,
        italics,
        ..Default::default()
    };
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            job.append("\n", 0.0, format(theme::text(), false));
        }
        for (range, color, italics) in spans(Some(&mut highlighter), line) {
            job.append(&line[range], 0.0, format(color, italics));
        }
    }

    if seen.len() >= 32 {
        seen.remove(0);
    }
    seen.push((key, job.clone()));
    Some(job)
}

/// One line of a file, highlighted for its type (by `path`), in the
/// diffs' colours; `muted` for lines that only give context. Lines are
/// highlighted on their own (as conflicts show them, in pieces), and kept:
/// they're drawn every frame.
pub fn line(path: &str, text: &str, size: f32, muted: bool) -> LayoutJob {
    type Key = (
        String,
        String,
        u32,
        bool,
        two_face::theme::EmbeddedThemeName,
    );
    static SEEN: OnceLock<Mutex<HashMap<Key, LayoutJob>>> = OnceLock::new();
    let syntax = syntax_for(path);
    let key: Key = (
        syntax.map_or_else(String::new, |s| s.name.clone()),
        text.to_string(),
        size.to_bits(),
        muted,
        theme::syntax(),
    );
    let seen = SEEN.get_or_init(Mutex::default);
    if let Some(job) = seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
    {
        return job.clone();
    }
    let mut highlighter = syntax.map(|s| HighlightLines::new(s, syntax_theme()));
    let mut job = LayoutJob::default();
    for (range, color, italics) in spans(highlighter.as_mut(), text) {
        job.append(
            &text[range],
            0.0,
            TextFormat {
                font_id: theme::mono(size),
                color: if muted {
                    color.gamma_multiply(0.6)
                } else {
                    color
                },
                italics,
                ..Default::default()
            },
        );
    }
    let mut seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
    if seen.len() > 8_000 {
        seen.clear();
    }
    seen.insert(key, job.clone());
    job
}

/// A whole file's text, highlighted for its type, as one block.
pub fn block(path: &str, text: &str, size: f32) -> LayoutJob {
    let mut highlighter = syntax_for(path)
        .filter(|_| text.len() <= MAX_HIGHLIGHT_BYTES)
        .map(|s| HighlightLines::new(s, syntax_theme()));
    let mut job = LayoutJob::default();
    let format = |color, italics| TextFormat {
        font_id: theme::mono(size),
        color,
        italics,
        ..Default::default()
    };
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            job.append("\n", 0.0, format(theme::text(), false));
        }
        for (range, color, italics) in spans(highlighter.as_mut(), line) {
            job.append(&line[range], 0.0, format(color, italics));
        }
    }
    job
}

/// A row of the unified layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Row {
    Header(usize),
    /// Hunk index, line index.
    Line(usize, usize),
}

/// A row of the side-by-side layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplitRow {
    Header(usize),
    Pair(Option<(usize, usize)>, Option<(usize, usize)>),
}

pub struct RenderedDiff {
    /// With the whole file, the hunks after the first `changes` are its
    /// unchanged lines.
    pub diff: FileDiff,
    /// How many of the hunks are the change's own.
    pub changes: usize,
    /// `jobs[hunk][line]`: the line's text, highlighted, with changed words marked.
    pub jobs: Vec<Vec<LayoutJob>>,
    pub unified: Vec<Row>,
    pub split: Vec<SplitRow>,
    /// Longest line, in characters.
    pub widest: usize,
}

impl RenderedDiff {
    pub fn new(diff: FileDiff) -> Self {
        Self::build(diff, Vec::new(), true)
    }

    /// The whole file: the diff with the unchanged lines around its hunks
    /// (`runs`, one before each hunk and one after the last), and hunk
    /// headers only where `headers` (they hold the staging buttons).
    pub fn whole(diff: FileDiff, runs: Vec<Vec<DiffLine>>, headers: bool) -> Self {
        Self::build(diff, runs, headers)
    }

    fn build(mut diff: FileDiff, runs: Vec<Vec<DiffLine>>, headers: bool) -> Self {
        let changes = diff.hunks.len();
        let runs = if runs.len() == changes + 1 {
            runs
        } else {
            Vec::new()
        };
        let whole = !runs.is_empty();
        // The unchanged runs go after the real hunks (whose numbering staging
        // relies on), as hunks of their own; the rows put them in place.
        diff.hunks.extend(runs.into_iter().map(|lines| Hunk {
            header: String::new(),
            lines,
        }));
        let bytes: usize = diff
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .map(|l| l.content.len())
            .sum();
        let syntax = if bytes <= MAX_HIGHLIGHT_BYTES {
            syntax_for(&diff.path)
        } else {
            None
        };
        let jobs = diff.hunks.iter().map(|h| render_hunk(h, syntax)).collect();
        let mut unified = Vec::new();
        let mut split = Vec::new();
        let unchanged = |at: usize, unified: &mut Vec<Row>, split: &mut Vec<SplitRow>| {
            if whole {
                let h = changes + at;
                for l in 0..diff.hunks[h].lines.len() {
                    unified.push(Row::Line(h, l));
                    split.push(SplitRow::Pair(Some((h, l)), Some((h, l))));
                }
            }
        };
        for (h, hunk) in diff.hunks[..changes].iter().enumerate() {
            unchanged(h, &mut unified, &mut split);
            if headers || !whole {
                unified.push(Row::Header(h));
                split.push(SplitRow::Header(h));
            }
            unified.extend((0..hunk.lines.len()).map(|l| Row::Line(h, l)));
            split.extend(
                split_rows(hunk)
                    .into_iter()
                    .map(|(a, b)| SplitRow::Pair(a.map(|l| (h, l)), b.map(|l| (h, l)))),
            );
        }
        unchanged(changes, &mut unified, &mut split);
        let widest = diff
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .map(|l| l.content.chars().count())
            .max()
            .unwrap_or(0);
        RenderedDiff {
            diff,
            changes,
            jobs,
            unified,
            split,
            widest,
        }
    }
}

/// Pairs a hunk's lines for side-by-side display: context on both sides,
/// each run of deletions matched line by line with the additions after it.
pub fn split_rows(hunk: &Hunk) -> Vec<(Option<usize>, Option<usize>)> {
    let mut rows = Vec::new();
    let mut dels: Vec<usize> = Vec::new();
    let mut adds: Vec<usize> = Vec::new();
    let mut last = None;
    let flush = |rows: &mut Vec<_>, dels: &mut Vec<usize>, adds: &mut Vec<usize>| {
        for i in 0..dels.len().max(adds.len()) {
            rows.push((dels.get(i).copied(), adds.get(i).copied()));
        }
        dels.clear();
        adds.clear();
    };
    for (i, line) in hunk.lines.iter().enumerate() {
        match line.kind {
            LineKind::Delete => {
                if !adds.is_empty() {
                    flush(&mut rows, &mut dels, &mut adds);
                }
                dels.push(i);
                last = Some(LineKind::Delete);
            }
            LineKind::Add => {
                adds.push(i);
                last = Some(LineKind::Add);
            }
            // The marker belongs to whichever side the previous line was on.
            LineKind::NoEol => match last {
                Some(LineKind::Delete) => dels.push(i),
                Some(LineKind::Add) => adds.push(i),
                _ => {
                    flush(&mut rows, &mut dels, &mut adds);
                    rows.push((Some(i), Some(i)));
                }
            },
            LineKind::Context => {
                flush(&mut rows, &mut dels, &mut adds);
                rows.push((Some(i), Some(i)));
                last = Some(LineKind::Context);
            }
        }
    }
    flush(&mut rows, &mut dels, &mut adds);
    rows
}

fn to_color(c: syntect::highlighting::Color) -> Color32 {
    Color32::from_rgb(c.r, c.g, c.b)
}

/// Colour spans (byte ranges) for one line, advancing the highlighter.
fn spans(
    highlighter: Option<&mut HighlightLines>,
    text: &str,
) -> Vec<(Range<usize>, Color32, bool)> {
    let Some(h) = highlighter else {
        return vec![(0..text.len(), theme::text(), false)];
    };
    let line = format!("{text}\n");
    let Ok(regions) = h.highlight_line(&line, syntaxes()) else {
        return vec![(0..text.len(), theme::text(), false)];
    };
    let mut out = Vec::new();
    let mut at = 0;
    for (style, piece) in regions {
        let end = (at + piece.len()).min(text.len());
        if at < end {
            out.push((
                at..end,
                to_color(style.foreground),
                style.font_style.contains(FontStyle::ITALIC),
            ));
        }
        at += piece.len();
    }
    out
}

fn render_hunk(hunk: &Hunk, syntax: Option<&SyntaxReference>) -> Vec<LayoutJob> {
    // Changed words of each paired line: (ranges, background).
    let mut marks: HashMap<usize, (Vec<Range<usize>>, Color32)> = HashMap::new();
    for (d, a, w) in words::pair_lines(hunk) {
        marks.insert(d, (w.old, theme::word_del()));
        marks.insert(a, (w.new, theme::word_add()));
    }
    // Each side is highlighted as continuous text, so constructs that span
    // lines (block comments, multi-line strings) stay correct.
    let mut old_side = syntax.map(|s| HighlightLines::new(s, syntax_theme()));
    let mut new_side = syntax.map(|s| HighlightLines::new(s, syntax_theme()));

    hunk.lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let text = line.content.as_str();
            let colours = match line.kind {
                LineKind::Context => {
                    spans(old_side.as_mut(), text);
                    spans(new_side.as_mut(), text)
                }
                LineKind::Delete => spans(old_side.as_mut(), text),
                LineKind::Add => spans(new_side.as_mut(), text),
                LineKind::NoEol => vec![(0..text.len(), theme::muted(), true)],
            };
            build_job(text, &colours, marks.get(&i))
        })
        .collect()
}

fn build_job(
    text: &str,
    colours: &[(Range<usize>, Color32, bool)],
    marks: Option<&(Vec<Range<usize>>, Color32)>,
) -> LayoutJob {
    let mut cuts: Vec<usize> = colours
        .iter()
        .flat_map(|(r, _, _)| [r.start, r.end])
        .collect();
    if let Some((ranges, _)) = marks {
        cuts.extend(ranges.iter().flat_map(|r| [r.start, r.end]));
    }
    cuts.extend([0, text.len()]);
    cuts.sort_unstable();
    cuts.dedup();

    let mut job = LayoutJob::default();
    for pair in cuts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if a >= b || b > text.len() {
            continue;
        }
        let (color, italics) = colours
            .iter()
            .find(|(r, _, _)| r.start <= a && a < r.end)
            .map_or((theme::text(), false), |(_, c, i)| (*c, *i));
        let background = marks
            .filter(|(ranges, _)| ranges.iter().any(|r| r.start <= a && a < r.end))
            .map_or(Color32::TRANSPARENT, |(_, bg)| *bg);
        job.append(
            &text[a..b],
            0.0,
            TextFormat {
                font_id: theme::mono(theme::MONO_SIZE),
                color,
                background,
                italics,
                ..Default::default()
            },
        );
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_raccoon_core::details::{DiffLine, FileStatus};

    fn line(kind: LineKind, content: &str) -> DiffLine {
        DiffLine {
            kind,
            old_line: None,
            new_line: None,
            content: content.into(),
        }
    }

    #[test]
    fn highlights_fenced_code_by_its_language() {
        let job = code("rust", "fn main() {\n    let x = 1;\n}").expect("rust");
        assert_eq!(job.text, "fn main() {\n    let x = 1;\n}");
        let colours: std::collections::HashSet<_> =
            job.sections.iter().map(|s| s.format.color).collect();
        assert!(colours.len() > 1, "more than one colour");
        assert!(code("yaml", "a: 1").is_some());
        assert!(code("no-such-language", "text").is_none());
    }

    #[test]
    fn finds_grammars_by_extension_and_name() {
        assert_eq!(
            syntax_for("src/main.rs").map(|s| s.name.as_str()),
            Some("Rust")
        );
        assert_eq!(
            syntax_for("web/App.tsx").map(|s| s.name.as_str()),
            Some("TypeScriptReact")
        );
        assert_eq!(
            syntax_for("Cargo.toml").map(|s| s.name.as_str()),
            Some("TOML")
        );
        assert!(syntax_for("LICENSE").is_none());
        assert!(syntax_for(".gitignore").is_none_or(|s| s.name != "Rust"));
    }

    #[test]
    fn pairs_rows_for_side_by_side() {
        let hunk = Hunk {
            header: "@@".into(),
            lines: vec![
                line(LineKind::Context, "a"),
                line(LineKind::Delete, "b"),
                line(LineKind::Add, "B"),
                line(LineKind::Add, "C"),
                line(LineKind::Context, "d"),
                line(LineKind::Add, "e"),
                line(LineKind::NoEol, ""),
            ],
        };
        assert_eq!(
            split_rows(&hunk),
            [
                (Some(0), Some(0)),
                (Some(1), Some(2)),
                (None, Some(3)),
                (Some(4), Some(4)),
                (None, Some(5)),
                (None, Some(6)),
            ]
        );
    }

    #[test]
    fn highlights_and_marks_changed_words_keeping_the_text() {
        let diff = FileDiff {
            path: "src/lib.rs".into(),
            old_path: None,
            status: FileStatus::Modified,
            binary: false,
            truncated: false,
            hunks: vec![Hunk {
                header: "@@".into(),
                lines: vec![
                    line(LineKind::Context, "/* a block"),
                    line(LineKind::Context, "   comment */ fn open() {"),
                    line(LineKind::Delete, "    let repo = discover(path)?;"),
                    line(
                        LineKind::Add,
                        "    let repo = discover(path).map_err(wrap)?;",
                    ),
                ],
            }],
        };
        let r = RenderedDiff::new(diff);
        let jobs = &r.jobs[0];
        for (job, line) in jobs.iter().zip(&r.diff.hunks[0].lines) {
            assert_eq!(job.text, line.content);
        }
        // The second line continues the block comment from the first.
        let comment_colour = jobs[0].sections[0].format.color;
        assert_eq!(jobs[1].sections[0].format.color, comment_colour);
        assert!(
            jobs[1].sections.len() > 1,
            "code after the comment is coloured separately"
        );
        // Only the added words are marked.
        let marked: String = jobs[3]
            .sections
            .iter()
            .filter(|s| s.format.background == theme::word_add())
            .map(|s| &jobs[3].text[s.byte_range.start.0..s.byte_range.end.0])
            .collect();
        assert_eq!(marked, ".map_err(wrap)");
        assert!(
            jobs[2]
                .sections
                .iter()
                .all(|s| s.format.background == Color32::TRANSPARENT)
        );
        assert_eq!(r.unified.len(), 5);
        assert_eq!(r.widest, 45);
    }
}
