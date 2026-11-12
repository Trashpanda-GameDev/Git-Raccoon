use serde::Serialize;

/// A short summary of a commit, as shown in the commit graph.
#[derive(Debug, Serialize)]
pub struct CommitSummary {
    pub id: String,
    pub parents: Vec<String>,
    pub summary: String,
    pub author: String,
    pub time: i64,
}

/// Walks history from HEAD and returns up to `limit` commits, newest first.
pub fn recent_commits(path: &str, limit: usize) -> Result<Vec<CommitSummary>, git2::Error> {
    let repo = git2::Repository::discover(path)?;
    let mut walk = repo.revwalk()?;
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;
    walk.push_head()?;

    walk.take(limit)
        .map(|oid| {
            let commit = repo.find_commit(oid?)?;
            Ok(CommitSummary {
                id: commit.id().to_string(),
                parents: commit.parent_ids().map(|p| p.to_string()).collect(),
                summary: commit.summary().unwrap_or_default().to_string(),
                author: commit.author().name().unwrap_or_default().to_string(),
                time: commit.time().seconds(),
            })
        })
        .collect()
}

#[tauri::command]
fn log(path: String, limit: Option<usize>) -> Result<Vec<CommitSummary>, String> {
    recent_commits(&path, limit.unwrap_or(500)).map_err(|e| e.message().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![log])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_commits_from_a_new_repo() {
        let dir = std::env::temp_dir().join(format!("git-raccoon-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = git2::Repository::init(&dir).unwrap();
        let sig = git2::Signature::now("Raccoon", "raccoon@example.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let first = repo.commit(Some("HEAD"), &sig, &sig, "first", &tree, &[]).unwrap();
        let parent = repo.find_commit(first).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "second", &tree, &[&parent]).unwrap();

        let commits = recent_commits(dir.to_str().unwrap(), 10).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].summary, "second");
        assert_eq!(commits[0].parents, vec![first.to_string()]);
        assert_eq!(commits[1].summary, "first");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
