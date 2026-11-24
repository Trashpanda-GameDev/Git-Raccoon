//! Commits are by the profile in use, without changing Git's config. Its
//! own test binary: the identity is the whole process's.

#![expect(
    clippy::unwrap_used,
    reason = "test setup: a failure should simply fail the test"
)]

use std::process::Command;

use git_raccoon_core::commit;

fn git(dir: &std::path::Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn commits_as_the_profile_and_leaves_gits_config_alone() {
    let dir = std::env::temp_dir().join(format!("git-raccoon-identity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["config", "user.name", "Repo Person"]);
    git(&dir, &["config", "user.email", "repo@example.com"]);
    let path = dir.to_string_lossy().to_string();

    std::fs::write(dir.join("a.txt"), "a").unwrap();
    git(&dir, &["add", "."]);
    commit::set_identity(Some(("Rocky Raccoon".into(), "rocky@acme.example".into())));
    commit::commit(&path, "As the profile", false).unwrap();
    assert_eq!(
        git(&dir, &["log", "-1", "--format=%an <%ae> / %cn <%ce>"]),
        "Rocky Raccoon <rocky@acme.example> / Rocky Raccoon <rocky@acme.example>"
    );
    // Git's config is as it was.
    assert_eq!(git(&dir, &["config", "user.email"]), "repo@example.com");

    // Without one, Git's own.
    commit::set_identity(None);
    std::fs::write(dir.join("b.txt"), "b").unwrap();
    git(&dir, &["add", "."]);
    commit::commit(&path, "As Git's", false).unwrap();
    assert_eq!(
        git(&dir, &["log", "-1", "--format=%ae"]),
        "repo@example.com"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
