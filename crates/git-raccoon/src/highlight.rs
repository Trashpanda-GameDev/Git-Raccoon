//! Turning a diff into syntax-highlighted, word-marked text ready to draw.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

use eframe::egui::Color32;
use eframe::egui::text::{LayoutJob, TextFormat};
use git_raccoon_core::details::{FileDiff, Hunk, LineKind};
use git_raccoon_core::words;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Theme};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use two_face::theme::EmbeddedThemeName;

use crate::theme;

/// Skip syntax highlighting for diffs with more text than this.
const MAX_HIGHLIGHT_BYTES: usize = 400_000;

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

/// The syntax colours for the current light or dark theme.
fn syntax_theme() -> &'static Theme {
    static DARK: OnceLock<Theme> = OnceLock::new();
    static LIGHT: OnceLock<Theme> = OnceLock::new();
    let (slot, name) = if theme::is_light() {
        (&LIGHT, EmbeddedThemeName::OneHalfLight)
    } else {
        (&DARK, EmbeddedThemeName::TwoDark)
    };
    slot.get_or_init(|| two_face::theme::extra().get(name).clone())
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
    pub diff: FileDiff,
    /// `jobs[hunk][line]`: the line's text, highlighted, with changed words marked.
    pub jobs: Vec<Vec<LayoutJob>>,
    pub unified: Vec<Row>,
    pub split: Vec<SplitRow>,
    /// Longest line, in characters.
    pub widest: usize,
}

impl RenderedDiff {
    pub fn new(diff: FileDiff) -> Self {
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
        for (h, hunk) in diff.hunks.iter().enumerate() {
            unified.push(Row::Header(h));
            unified.extend((0..hunk.lines.len()).map(|l| Row::Line(h, l)));
            split.push(SplitRow::Header(h));
            split.extend(
                split_rows(hunk)
                    .into_iter()
                    .map(|(a, b)| SplitRow::Pair(a.map(|l| (h, l)), b.map(|l| (h, l)))),
            );
        }
        let widest = diff
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .map(|l| l.content.chars().count())
            .max()
            .unwrap_or(0);
        RenderedDiff {
            diff,
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
