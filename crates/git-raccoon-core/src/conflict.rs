//! Resolving conflicted files: reading Git's conflict markers, picking a
//! side for each conflict, or taking one side's whole file.

use git2::Repository;

use crate::{Error, Result, cli};

/// A piece of a conflicted file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chunk {
    /// Lines both sides agree on.
    Same(String),
    Conflict(Conflict),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// After "<<<<<<<", usually "HEAD".
    pub ours_label: String,
    pub ours: String,
    /// The common ancestor's version, with diff3-style markers.
    pub base: Option<String>,
    /// After ">>>>>>>", usually the branch being merged in.
    pub theirs_label: String,
    pub theirs: String,
}

/// Which version of a conflict to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Ours,
    Theirs,
    /// Ours, then theirs.
    Both,
}

fn marker(line: &str, sign: char) -> Option<&str> {
    let body = line.trim_end_matches(['\n', '\r']);
    let rest = body.strip_prefix(&sign.to_string().repeat(7))?;
    if rest.is_empty() {
        Some("")
    } else {
        rest.strip_prefix(' ')
    }
}

/// Splits a file into agreed text and conflicts. `None` if it has no
/// (complete) conflict markers.
pub fn parse(text: &str) -> Option<Vec<Chunk>> {
    let mut chunks = Vec::new();
    let mut same = String::new();
    let mut lines = text.split_inclusive('\n');
    let mut found = false;
    while let Some(line) = lines.next() {
        let Some(ours_label) = marker(line, '<') else {
            same.push_str(line);
            continue;
        };
        // Inside a conflict: ours, maybe base, then theirs.
        let (mut ours, mut base, mut theirs) = (String::new(), None::<String>, String::new());
        let mut part = 0; // 0 ours, 1 base, 2 theirs
        let mut theirs_label = None;
        for line in lines.by_ref() {
            if part == 0 && marker(line, '|').is_some() {
                part = 1;
                base = Some(String::new());
            } else if part < 2 && marker(line, '=') == Some("") {
                part = 2;
            } else if part == 2
                && let Some(label) = marker(line, '>')
            {
                theirs_label = Some(label.to_string());
                break;
            } else {
                match part {
                    0 => ours.push_str(line),
                    1 => base.get_or_insert_default().push_str(line),
                    _ => theirs.push_str(line),
                }
            }
        }
        let theirs_label = theirs_label?; // unfinished markers: not a conflict file
        if !same.is_empty() {
            chunks.push(Chunk::Same(std::mem::take(&mut same)));
        }
        chunks.push(Chunk::Conflict(Conflict {
            ours_label: ours_label.to_string(),
            ours,
            base,
            theirs_label,
            theirs,
        }));
        found = true;
    }
    if !same.is_empty() {
        chunks.push(Chunk::Same(same));
    }
    found.then_some(chunks)
}

/// The file with conflict number `index` (counting conflicts only) replaced
/// by the version picked.
pub fn resolve(chunks: &[Chunk], index: usize, pick: Pick) -> String {
    let mut out = String::new();
    let mut n = 0;
    for chunk in chunks {
        match chunk {
            Chunk::Same(text) => out.push_str(text),
            Chunk::Conflict(c) if n == index => {
                n += 1;
                match pick {
                    Pick::Ours => out.push_str(&c.ours),
                    Pick::Theirs => out.push_str(&c.theirs),
                    Pick::Both => {
                        out.push_str(&c.ours);
                        out.push_str(&c.theirs);
                    }
                }
            }
            Chunk::Conflict(c) => {
                n += 1;
                out.push_str(
                    &format!("<<<<<<< {}\n", c.ours_label).replace("<<<<<<< \n", "<<<<<<<\n"),
                );
                out.push_str(&c.ours);
                if let Some(base) = &c.base {
                    out.push_str("|||||||\n");
                    out.push_str(base);
                }
                out.push_str("=======\n");
                out.push_str(&c.theirs);
                out.push_str(
                    &format!(">>>>>>> {}\n", c.theirs_label).replace(">>>>>>> \n", ">>>>>>>\n"),
                );
            }
        }
    }
    out
}

fn full_path(path: &str, file: &str) -> Result<std::path::PathBuf> {
    let repo = Repository::discover(path)?;
    let root = repo
        .workdir()
        .ok_or_else(|| Error::Unsupported("a bare repository has no files".into()))?;
    Ok(root.join(file))
}

/// The file's current text, for the conflict view.
pub fn read(path: &str, file: &str) -> Result<String> {
    std::fs::read_to_string(full_path(path, file)?)
        .map_err(|e| Error::Unsupported(format!("can't read {file}: {e}")))
}

/// Writes the file, but only if it still reads `expected` (so edits made
/// elsewhere in the meantime aren't overwritten).
pub fn write(path: &str, file: &str, expected: &str, text: &str) -> Result<()> {
    if read(path, file)? != expected {
        return Err(Error::Stale(format!("{file} changed on disk")));
    }
    std::fs::write(full_path(path, file)?, text)
        .map_err(|e| Error::Unsupported(format!("can't write {file}: {e}")))
}

/// Which sides of a conflicted file exist (one may have deleted it).
fn sides(path: &str, file: &str) -> Result<(bool, bool)> {
    let repo = Repository::discover(path)?;
    let index = repo.index()?;
    for c in index.conflicts()? {
        let c = c?;
        let entry = c.our.as_ref().or(c.their.as_ref()).or(c.ancestor.as_ref());
        if entry.is_some_and(|e| e.path == file.as_bytes()) {
            return Ok((c.our.is_some(), c.their.is_some()));
        }
    }
    Err(Error::Stale(format!("{file} isn't conflicted")))
}

/// Resolves the whole file by taking one side's version (deleting it, if
/// that side deleted it).
pub fn take_side(path: &str, file: &str, ours: bool) -> Result<()> {
    let (has_ours, has_theirs) = sides(path, file)?;
    if (ours && has_ours) || (!ours && has_theirs) {
        let side = if ours { "--ours" } else { "--theirs" };
        cli::run(path, &["checkout", side, "--", file], None)?;
        cli::run(path, &["add", "--", file], None).map(drop)
    } else {
        cli::run(path, &["rm", "--quiet", "--", file], None).map(drop)
    }
}

/// Marks a file resolved (stages it as it is now). Refuses while conflict
/// markers remain.
pub fn mark_resolved(path: &str, file: &str) -> Result<()> {
    if let Ok(text) = read(path, file)
        && parse(&text).is_some()
    {
        return Err(Error::Unsupported(format!(
            "{file} still has conflict markers"
        )));
    }
    cli::run(path, &["add", "--", file], None).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::tests::{conflicting, git};
    use crate::merge::{MergeMode, has_conflicts, merge};

    const SAMPLE: &str = "a\n<<<<<<< HEAD\nours 1\nours 2\n=======\ntheirs\n>>>>>>> feature\nb\n<<<<<<< HEAD\nx\n||||||| base\nbase\n=======\ny\n>>>>>>> feature\n";

    #[test]
    fn parses_conflicts_including_diff3() {
        let chunks = parse(SAMPLE).unwrap();
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0], Chunk::Same("a\n".into()));
        let Chunk::Conflict(c) = &chunks[1] else {
            panic!()
        };
        assert_eq!(
            (c.ours.as_str(), c.theirs.as_str()),
            ("ours 1\nours 2\n", "theirs\n")
        );
        assert_eq!(
            (c.ours_label.as_str(), c.theirs_label.as_str()),
            ("HEAD", "feature")
        );
        let Chunk::Conflict(c) = &chunks[3] else {
            panic!()
        };
        assert_eq!(c.base.as_deref(), Some("base\n"));
        assert!(parse("no markers\n").is_none());
        assert!(parse("<<<<<<< HEAD\nunfinished\n").is_none());
    }

    #[test]
    fn resolves_one_conflict_at_a_time() {
        let chunks = parse(SAMPLE).unwrap();
        let once = resolve(&chunks, 0, Pick::Theirs);
        assert!(once.starts_with("a\ntheirs\nb\n<<<<<<< HEAD\n"), "{once}");
        let twice = resolve(&parse(&once).unwrap(), 0, Pick::Both);
        assert_eq!(twice, "a\ntheirs\nb\nx\ny\n");
        assert!(parse(&twice).is_none());
        assert_eq!(
            resolve(&parse(SAMPLE).unwrap(), 1, Pick::Ours)
                .matches("<<<<<<<")
                .count(),
            1
        );
    }

    #[test]
    fn resolves_files_in_a_real_merge() {
        let (t, _) = conflicting();
        merge(t.path(), "feature", MergeMode::Default).unwrap();
        let text = read(t.path(), "f.txt").unwrap();
        let chunks = parse(&text).unwrap();
        let fixed = resolve(&chunks, 0, Pick::Both);
        assert!(mark_resolved(t.path(), "f.txt").is_err(), "markers remain");
        assert!(
            write(t.path(), "f.txt", "not what's there", &fixed).is_err(),
            "stale"
        );
        write(t.path(), "f.txt", &text, &fixed).unwrap();
        mark_resolved(t.path(), "f.txt").unwrap();
        assert!(!has_conflicts(t.path()).unwrap());
        assert_eq!(
            read(t.path(), "f.txt").unwrap(),
            "one\ntwo (main)\nTWO (feature)\nthree\n"
        );

        // Whole-file picks, again from the start.
        git(&t, &["merge", "--abort"]);
        merge(t.path(), "feature", MergeMode::Default).unwrap();
        take_side(t.path(), "f.txt", false).unwrap();
        assert_eq!(
            read(t.path(), "f.txt").unwrap(),
            "one\nTWO (feature)\nthree\n"
        );
        assert!(!has_conflicts(t.path()).unwrap());
    }

    #[test]
    fn takes_a_deletion_as_a_side() {
        let (t, _) = conflicting();
        git(&t, &["rm", "-q", "f.txt"]);
        git(&t, &["commit", "-q", "-m", "delete f on main"]);
        merge(t.path(), "feature", MergeMode::Default).unwrap();
        take_side(t.path(), "f.txt", true).unwrap();
        assert!(!t.dir.join("f.txt").exists(), "ours deleted it");
        assert!(!has_conflicts(t.path()).unwrap());
    }
}
