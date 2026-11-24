//! A pull/merge request's commits, here: fetched without making a branch
//! for them, so its files can be shown whole and blamed, and compared.

use git2::{DiffFindOptions, DiffOptions, Oid, Repository};

use crate::details::{FileDiff, patch_lines};
use crate::{Error, Result, cli};

/// Fetches `refs` from `remote` (a request's head ref, and the branch it
/// goes into), keeping their commits but making no branch for them. Never
/// asks for a password: a remote that needs one fails instead.
pub fn fetch(path: &str, remote: &str, refs: &[&str]) -> Result<()> {
    let mut args = vec!["fetch", "--no-tags", "--quiet", remote];
    args.extend_from_slice(refs);
    cli::run_quiet(path, &args).map(drop)
}

/// Whether commit `id` is here.
pub fn has(path: &str, id: &str) -> bool {
    Repository::discover(path)
        .ok()
        .zip(Oid::from_str(id).ok())
        .is_some_and(|(repo, oid)| repo.find_commit(oid).is_ok())
}

/// Where `a` and `b` last met (what a request's changes are measured from).
pub fn merge_base(path: &str, a: &str, b: &str) -> Result<String> {
    let repo = Repository::discover(path)?;
    let commit =
        |rev: &str| -> Result<Oid> { Ok(repo.revparse_single(rev)?.peel_to_commit()?.id()) };
    Ok(repo.merge_base(commit(a)?, commit(b)?)?.to_string())
}

/// A file's contents at a commit, or `None` if it isn't there.
pub fn file_at(path: &str, id: &str, file: &str) -> Result<Option<Vec<u8>>> {
    let repo = Repository::discover(path)?;
    let tree = repo.revparse_single(id)?.peel_to_commit()?.tree()?;
    let Ok(entry) = tree.get_path(std::path::Path::new(file)) else {
        return Ok(None);
    };
    let blob = entry
        .to_object(&repo)?
        .into_blob()
        .map_err(|_| Error::Command(format!("{file} isn't a file there")))?;
    Ok(Some(blob.content().to_vec()))
}

/// A file's two versions for a preview: `old_file` at `from` and `file` at
/// `to` (a side is `None` where it isn't there).
pub fn blobs_between(
    path: &str,
    from: &str,
    to: &str,
    old_file: &str,
    file: &str,
) -> Result<crate::details::FileBlobs> {
    let side = |id: &str, file: &str| -> Result<Option<crate::details::BlobData>> {
        Ok(
            file_at(path, id, file)?.map(|bytes| crate::details::BlobData {
                size: bytes.len(),
                bytes: (bytes.len() <= crate::details::MAX_BLOB_BYTES).then_some(bytes),
            }),
        )
    };
    Ok(crate::details::FileBlobs {
        old: side(from, old_file)?,
        new: side(to, file)?,
    })
}

/// What changed from `from` to `to` (each a commit or branch), file by file.
pub fn changes(path: &str, from: &str, to: &str) -> Result<Vec<FileDiff>> {
    let repo = Repository::discover(path)?;
    let tree = |rev: &str| -> Result<git2::Tree<'_>> {
        Ok(repo.revparse_single(rev)?.peel_to_commit()?.tree()?)
    };
    let (old, new) = (tree(from)?, tree(to)?);
    let mut diff = repo.diff_tree_to_tree(
        Some(&old),
        Some(&new),
        Some(DiffOptions::new().context_lines(3)),
    )?;
    diff.find_similar(Some(DiffFindOptions::new().renames(true)))?;
    (0..diff.deltas().len())
        .map(|i| patch_lines(&diff, i))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    #[test]
    fn compares_and_reads_files_at_commits() {
        let t = TempRepo::new();
        t.write("a.txt", "one\ntwo\n");
        let first = t.commit_all("first");
        t.write("a.txt", "one\n2\n");
        t.write("b.txt", "new\n");
        let second = t.commit_all("second");
        let (first, second) = (first.to_string(), second.to_string());
        assert!(has(t.path(), &first));
        assert!(!has(t.path(), "0123456789012345678901234567890123456789"));
        assert_eq!(merge_base(t.path(), &first, &second).unwrap(), first);
        assert_eq!(
            file_at(t.path(), &first, "a.txt").unwrap().as_deref(),
            Some(&b"one\ntwo\n"[..])
        );
        assert_eq!(file_at(t.path(), &first, "b.txt").unwrap(), None);
        let files = changes(t.path(), &first, &second).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["a.txt", "b.txt"]);
        assert_eq!(files[0].hunks[0].lines.len(), 3);
    }
}
