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
    let Some(c) = conflicts(chunks).nth(index) else {
        return resolve_with(chunks, usize::MAX, "");
    };
    let text = match pick {
        Pick::Ours => c.ours.clone(),
        Pick::Theirs => c.theirs.clone(),
        Pick::Both => format!("{}{}", c.ours, c.theirs),
    };
    resolve_with(chunks, index, &text)
}

/// The conflicts among `chunks`, in order.
pub fn conflicts(chunks: &[Chunk]) -> impl Iterator<Item = &Conflict> {
    chunks.iter().filter_map(|c| match c {
        Chunk::Conflict(c) => Some(c),
        Chunk::Same(_) => None,
    })
}

/// The file with conflict number `index` replaced by `text` (lines picked
/// from either side, or written by hand); the other conflicts stay.
pub fn resolve_with(chunks: &[Chunk], index: usize, text: &str) -> String {
    resolve_many(
        chunks,
        &std::collections::BTreeMap::from([(index, text.to_string())]),
    )
}

/// The file with each conflict in `results` (by number, counting conflicts
/// only) replaced by its text; the others stay, with their markers.
pub fn resolve_many(
    chunks: &[Chunk],
    results: &std::collections::BTreeMap<usize, String>,
) -> String {
    let mut out = String::new();
    let mut n = 0;
    for chunk in chunks {
        match chunk {
            Chunk::Same(same) => out.push_str(same),
            Chunk::Conflict(c) => {
                if let Some(text) = results.get(&n) {
                    out.push_str(text);
                    if !text.is_empty() && !text.ends_with('\n') {
                        out.push('\n');
                    }
                } else {
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
                n += 1;
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

/// How a file came to conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Both sides changed it (it has conflict markers, unless binary).
    BothChanged,
    /// Both sides added a file at this path.
    BothAdded,
    /// What the files had ("ours") deleted it; the incoming side changed it.
    DeletedOnOurs,
    /// The incoming side ("theirs") deleted it; ours changed it.
    DeletedOnTheirs,
    /// Anything else (a rename on one side, say).
    Other,
}

/// A conflicted file: how it conflicts, and how many conflicts its text
/// has left (0 for a deleted side, or a binary file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictedFile {
    pub path: String,
    pub kind: Kind,
    pub conflicts: usize,
    /// Its text can't be read as text (binary, or not there).
    pub binary: bool,
    /// Git resolved it the way it was resolved before (rerere): to check,
    /// then mark resolved.
    pub remembered: bool,
}

/// Turns remembering how conflicts are resolved (Git's rerere) on or off
/// for the app's Git, leaving the user's own config alone.
pub fn set_remember(on: bool) {
    cli::set_remember(on);
}

/// The files with conflicts, in path order.
pub fn files(path: &str) -> Result<Vec<ConflictedFile>> {
    let repo = Repository::discover(path)?;
    let index = repo.index()?;
    let mut out = Vec::new();
    for c in index.conflicts()? {
        let c = c?;
        let entry = c.our.as_ref().or(c.their.as_ref()).or(c.ancestor.as_ref());
        let Some(entry) = entry else { continue };
        let file = String::from_utf8_lossy(&entry.path).into_owned();
        let kind = match (c.ancestor.is_some(), c.our.is_some(), c.their.is_some()) {
            (true, true, true) => Kind::BothChanged,
            (false, true, true) => Kind::BothAdded,
            (true, false, true) => Kind::DeletedOnOurs,
            (true, true, false) => Kind::DeletedOnTheirs,
            _ => Kind::Other,
        };
        let text = read(path, &file).ok();
        let conflicts = text
            .as_deref()
            .and_then(parse)
            .map_or(0, |chunks| self::conflicts(&chunks).count());
        out.push(ConflictedFile {
            path: file,
            kind,
            conflicts,
            binary: text.is_none(),
            remembered: false,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    // Text files left without markers may be rerere's work: when it's
    // taking part (its MERGE_RR is there), those it didn't leave for the
    // person ("remaining" lists the ones it couldn't resolve, and ones
    // resolved by hand).
    if out.iter().any(|f| f.conflicts == 0 && !f.binary) && repo.path().join("MERGE_RR").exists() {
        let remaining: Vec<String> = cli::run(path, &["rerere", "remaining"], None)
            .map(|o| o.lines().map(str::to_string).collect())
            .unwrap_or_default();
        for f in &mut out {
            f.remembered = f.conflicts == 0
                && !f.binary
                && matches!(f.kind, Kind::BothChanged | Kind::BothAdded)
                && !remaining.contains(&f.path);
        }
    }
    Ok(out)
}

/// Forgets how `file`'s conflict was resolved before, and brings its
/// conflict back to resolve by hand.
pub fn forget(path: &str, file: &str) -> Result<()> {
    cli::run(path, &["rerere", "forget", "--", file], None)?;
    cli::run(path, &["checkout", "--merge", "--", file], None).map(drop)
}

/// Fills in each conflict's common ancestor ("base"), which Git's markers
/// only carry with `merge.conflictStyle = diff3`: from Git's own copies of
/// the three versions, merged again with the base shown, without touching
/// the file. Conflicts that can't be matched up (edited since) keep none.
pub fn with_bases(path: &str, file: &str, chunks: &mut [Chunk]) {
    if conflicts(chunks).all(|c| c.base.is_some()) {
        return;
    }
    let Some(again) = merged_with_bases(path, file) else {
        return;
    };
    let mut known: Vec<Conflict> = conflicts(&again).cloned().collect();
    for chunk in chunks.iter_mut() {
        let Chunk::Conflict(c) = chunk else { continue };
        if c.base.is_some() {
            continue;
        }
        if let Some(i) = known
            .iter()
            .position(|k| k.ours == c.ours && k.theirs == c.theirs)
        {
            c.base = known.remove(i).base;
        }
    }
}

/// The file merged again from Git's three copies, with bases shown.
fn merged_with_bases(path: &str, file: &str) -> Option<Vec<Chunk>> {
    let repo = Repository::discover(path).ok()?;
    let index = repo.index().ok()?;
    let c = index.conflicts().ok()?.flatten().find(|c| {
        let e = c.our.as_ref().or(c.their.as_ref());
        e.is_some_and(|e| e.path == file.as_bytes())
    })?;
    let (base, ours, theirs) = (c.ancestor?, c.our?, c.their?);
    let mut options = git2::MergeFileOptions::new();
    options.style_diff3(true);
    let merged = repo
        .merge_file_from_index(&base, &ours, &theirs, Some(&mut options))
        .ok()?;
    parse(std::str::from_utf8(merged.content()).ok()?)
}

/// Brings a resolved file's conflict back (its markers, and unresolved in
/// the index), from what Git kept when it was resolved.
pub fn unresolve(path: &str, file: &str) -> Result<()> {
    // Back to conflicted in the index (any kind of conflict), then, where
    // both sides have the file, its text with the markers again.
    cli::run(path, &["update-index", "--unresolve", "--", file], None)?;
    match sides(path, file) {
        Ok((true, true)) => cli::run(path, &["checkout", "--merge", "--", file], None).map(drop),
        Ok(_) => Ok(()),
        Err(_) => Err(Error::Stale(format!(
            "{file}'s conflict can't be brought back (Git no longer has it)"
        ))),
    }
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
    cli::run(path, &["add", "--", file], None)?;
    // Recorded now, so the same conflict is resolved this way next time
    // (when remembering).
    if cli::remembering() {
        let _ = cli::run(path, &["rerere"], None);
    }
    Ok(())
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
    fn resolves_several_conflicts_at_once() {
        let chunks = parse(SAMPLE).unwrap();
        let both = resolve_many(
            &chunks,
            &std::collections::BTreeMap::from([(0, "T\n".to_string()), (1, "XY".to_string())]),
        );
        assert_eq!(both, "a\nT\nb\nXY\n");
        let one = resolve_many(
            &chunks,
            &std::collections::BTreeMap::from([(1, String::new())]),
        );
        assert!(
            one.starts_with("a\n<<<<<<< HEAD\nours 1\n") && one.ends_with("b\n"),
            "{one}"
        );
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
    fn writes_a_conflict_as_given() {
        let chunks = parse(SAMPLE).unwrap();
        // Lines picked from both sides, in an order of one's choosing.
        let picked = resolve_with(&chunks, 0, "theirs\nours 2");
        assert!(
            picked.starts_with("a\ntheirs\nours 2\nb\n<<<<<<<"),
            "{picked}"
        );
        assert_eq!(conflicts(&parse(&picked).unwrap()).count(), 1);
        // Nothing at all is a choice too.
        assert!(resolve_with(&chunks, 0, "").starts_with("a\nb\n"));
    }

    #[test]
    fn lists_conflicted_files_with_how_they_conflict() {
        // f.txt changed on both sides; d.txt changed on main, deleted on feature.
        let t = crate::testutil::TempRepo::new();
        crate::merge::tests::identity(&t);
        t.write("f.txt", "one\ntwo\n");
        t.write("d.txt", "start\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "base"]);
        let main = t.repo.head().unwrap().shorthand().unwrap().to_string();
        git(&t, &["switch", "-q", "-c", "feature"]);
        t.write("f.txt", "one\nTWO\n");
        git(&t, &["rm", "-q", "d.txt"]);
        git(&t, &["commit", "-q", "-am", "feature"]);
        git(&t, &["switch", "-q", &main]);
        t.write("f.txt", "one\n2\n");
        t.write("d.txt", "changed on main\n");
        git(&t, &["commit", "-q", "-am", "main"]);
        merge(t.path(), "feature", MergeMode::Default).unwrap();
        let found = files(t.path()).unwrap();
        let kinds: Vec<(&str, Kind, usize)> = found
            .iter()
            .map(|f| (f.path.as_str(), f.kind, f.conflicts))
            .collect();
        assert_eq!(
            kinds,
            [
                ("d.txt", Kind::DeletedOnTheirs, 0),
                ("f.txt", Kind::BothChanged, 1)
            ]
        );
    }

    #[test]
    fn remembers_resolutions_and_forgets_them() {
        let (t, _) = conflicting();
        git(&t, &["config", "rerere.enabled", "true"]);
        merge(t.path(), "feature", MergeMode::Default).unwrap();
        assert!(!files(t.path()).unwrap()[0].remembered);
        let text = read(t.path(), "f.txt").unwrap();
        write(t.path(), "f.txt", &text, "one\ntwo (both)\nthree\n").unwrap();
        mark_resolved(t.path(), "f.txt").unwrap();
        crate::merge::continue_op(t.path()).unwrap();
        // The same merge again: resolved as before, still to be marked.
        git(&t, &["reset", "-q", "--hard", "HEAD~1"]);
        merge(t.path(), "feature", MergeMode::Default).unwrap();
        assert_eq!(read(t.path(), "f.txt").unwrap(), "one\ntwo (both)\nthree\n");
        let f = &files(t.path()).unwrap()[0];
        assert!(f.remembered && f.conflicts == 0, "{f:?}");
        // Redone by hand: the conflict is back.
        forget(t.path(), "f.txt").unwrap();
        let f = &files(t.path()).unwrap()[0];
        assert!(!f.remembered && f.conflicts == 1, "{f:?}");
    }

    #[test]
    fn finds_bases_without_diff3_and_brings_conflicts_back() {
        let (t, _) = conflicting();
        // Git's default style: no bases in the markers.
        git(&t, &["config", "merge.conflictStyle", "merge"]);
        merge(t.path(), "feature", MergeMode::Default).unwrap();
        let text = read(t.path(), "f.txt").unwrap();
        assert!(!text.contains("|||||||"));
        let mut chunks = parse(&text).unwrap();
        with_bases(t.path(), "f.txt", &mut chunks);
        let c = conflicts(&chunks).next().unwrap();
        assert_eq!(c.base.as_deref(), Some("two\n"));
        // The file itself is untouched.
        assert_eq!(read(t.path(), "f.txt").unwrap(), text);

        // Resolved, then brought back.
        take_side(t.path(), "f.txt", true).unwrap();
        assert!(!has_conflicts(t.path()).unwrap());
        unresolve(t.path(), "f.txt").unwrap();
        assert!(has_conflicts(t.path()).unwrap());
        assert!(read(t.path(), "f.txt").unwrap().contains("<<<<<<<"));
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
