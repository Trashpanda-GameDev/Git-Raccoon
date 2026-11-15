//! Ignoring files: adding patterns to the repository's `.gitignore`.

use std::io::Write as _;

use git2::Repository;

use crate::{Error, Result};

/// Adds `patterns` (paths from the repository's root, like "build/" or
/// "notes/todo.txt") to the `.gitignore` at the root, each anchored to the
/// root and once only. Returns the ones added.
pub fn add(path: &str, patterns: &[String]) -> Result<Vec<String>> {
    let repo = Repository::discover(path)?;
    let root = repo
        .workdir()
        .ok_or_else(|| Error::Unsupported("a bare repository has no working tree".into()))?
        .to_path_buf();
    let file = root.join(".gitignore");
    let existing = std::fs::read_to_string(&file).unwrap_or_default();
    let have: Vec<&str> = existing.lines().map(str::trim).collect();
    let mut added = Vec::new();
    for p in patterns {
        let line = format!("/{}", p.trim_start_matches('/'));
        if !have.contains(&line.as_str()) && !added.contains(&line) {
            added.push(line);
        }
    }
    if added.is_empty() {
        return Ok(added);
    }
    let mut text = String::new();
    if !existing.is_empty() && !existing.ends_with('\n') {
        text.push('\n');
    }
    for line in &added {
        text.push_str(line);
        text.push('\n');
    }
    let io = |e: std::io::Error| Error::Command(format!("could not write .gitignore: {e}"));
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .map_err(io)?
        .write_all(text.as_bytes())
        .map_err(io)?;
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;
    use crate::working::working_status;

    #[test]
    fn adds_patterns_once_and_the_files_drop_out_of_the_changes() {
        let t = TempRepo::new();
        t.write("keep.txt", "keep\n");
        t.write("notes/a.txt", "a\n");
        t.write("debug.log", "x\n");
        t.write(".gitignore", "*.tmp");
        let added = add(t.path(), &["notes/".into(), "debug.log".into()]).unwrap();
        assert_eq!(added, ["/notes/", "/debug.log"]);
        assert_eq!(
            std::fs::read_to_string(t.dir.join(".gitignore")).unwrap(),
            "*.tmp\n/notes/\n/debug.log\n"
        );
        // Again: nothing new.
        assert!(add(t.path(), &["debug.log".into()]).unwrap().is_empty());
        let status = working_status(t.path()).unwrap().unwrap();
        let paths: Vec<&str> = status.unstaged.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"keep.txt"));
        assert!(
            !paths
                .iter()
                .any(|p| p.starts_with("notes") || *p == "debug.log")
        );
    }
}
