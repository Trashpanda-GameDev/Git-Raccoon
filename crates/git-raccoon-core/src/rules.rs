//! Rules for some files: file names (patterns) that use a tool of their
//! own. Written as Git's attributes, so Git itself follows them:
//!
//! - every rule marks its files `raccoon-tool=<tool>`, which the conflict
//!   editor's "Open in…" uses for them;
//! - a rule that works *automatically while merging* also names a merge
//!   driver, `merge=raccoon-<tool>`, defined in `.git/config` as
//!   `git-raccoon merge-driver <tool> %O %A %B %P`: the tool merges what it
//!   can during the merge, and only what it can't is left as a conflict.
//!
//! A rule for *just me* lives in `.git/info/attributes`; one for *everyone*
//! adds its lines to `.gitattributes`, to commit. The driver's command
//! (with this computer's path to Git Raccoon) always stays in `.git/config`.

use std::path::{Path, PathBuf};

use git2::{ConfigLevel, Repository};

use crate::tools::{self, Places};
use crate::{Error, Result};

const TOOL_ATTR: &str = "raccoon-tool";
const DRIVER_PREFIX: &str = "raccoon-";

/// A rule: these files, this tool, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// File name patterns, as in `.gitattributes` ("*.unity", "Assets/**").
    pub patterns: Vec<String>,
    /// The tool's id (a preset's).
    pub tool: String,
    /// Merges them during the merge itself (a merge driver), or only when
    /// the file is opened from the conflict editor.
    pub automatic: bool,
    /// In `.gitattributes`, for everyone on the repository.
    pub everyone: bool,
}

impl Rule {
    fn driver(&self) -> String {
        format!("{DRIVER_PREFIX}{}", self.tool)
    }

    /// Its lines in an attributes file.
    pub fn lines(&self) -> Vec<String> {
        self.patterns
            .iter()
            .map(|p| {
                if self.automatic {
                    format!("{p} merge={} {TOOL_ATTR}={}", self.driver(), self.tool)
                } else {
                    format!("{p} {TOOL_ATTR}={}", self.tool)
                }
            })
            .collect()
    }

    pub fn tool_name(&self) -> String {
        tools::preset_by_id(&self.tool).map_or_else(|| self.tool.clone(), |p| p.name.to_string())
    }
}

/// The patterns a line of text holds ("*.unity *.prefab" or one per line).
pub fn split_patterns(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(str::to_string)
        .filter(|p| !p.starts_with('#'))
        .collect()
}

struct Files {
    /// `.git/info/attributes` (shared by worktrees).
    mine: PathBuf,
    /// `.gitattributes` at the top of the working tree.
    shared: PathBuf,
}

fn files(repo: &Repository) -> Result<Files> {
    let root = repo
        .workdir()
        .ok_or_else(|| Error::Unsupported("a bare repository has no files".into()))?;
    Ok(Files {
        mine: repo.commondir().join("info").join("attributes"),
        shared: root.join(".gitattributes"),
    })
}

fn read_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// A line's pattern and its tool, if it's one of ours: (pattern, tool,
/// automatic).
fn parse_line(line: &str) -> Option<(String, String, bool)> {
    let mut words = line.split_whitespace();
    let pattern = words.next().filter(|p| !p.starts_with('#'))?;
    let attrs: Vec<&str> = words.collect();
    let tool = attrs
        .iter()
        .find_map(|a| a.strip_prefix(&format!("{TOOL_ATTR}=")))?
        .to_string();
    let automatic = attrs
        .iter()
        .any(|a| *a == format!("merge={DRIVER_PREFIX}{tool}"));
    Some((pattern.to_string(), tool, automatic))
}

/// The repository's rules, in the order their first lines come (just me's
/// first).
pub fn rules(path: &str) -> Result<Vec<Rule>> {
    let repo = Repository::discover(path)?;
    let f = files(&repo)?;
    let mut out: Vec<Rule> = Vec::new();
    for (file, everyone) in [(&f.mine, false), (&f.shared, true)] {
        for line in read_lines(file) {
            let Some((pattern, tool, automatic)) = parse_line(&line) else {
                continue;
            };
            match out
                .iter_mut()
                .find(|r| r.tool == tool && r.automatic == automatic && r.everyone == everyone)
            {
                Some(r) => r.patterns.push(pattern),
                None => out.push(Rule {
                    patterns: vec![pattern],
                    tool,
                    automatic,
                    everyone,
                }),
            }
        }
    }
    Ok(out)
}

/// What saving a rule writes: each file, and the lines added to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// As shown: ".git/info/attributes", ".gitattributes", ".git/config".
    pub file: String,
    pub lines: Vec<String>,
}

/// The command Git runs for a rule's driver: this program, by its path.
pub fn driver_command(exe: &Path, tool: &str) -> String {
    // Git runs it with a shell: forward slashes and quotes keep a path with
    // spaces (or Windows' backslashes) whole.
    let exe = exe.display().to_string().replace('\\', "/");
    format!("\"{exe}\" merge-driver {tool} %O %A %B %P")
}

/// What adding `rule` would write (nothing is written).
pub fn preview(rule: &Rule, exe: &Path) -> Vec<Change> {
    let mut out = vec![Change {
        file: if rule.everyone {
            ".gitattributes".into()
        } else {
            ".git/info/attributes".into()
        },
        lines: rule.lines(),
    }];
    if rule.automatic {
        out.push(Change {
            file: ".git/config".into(),
            lines: vec![
                format!("[merge \"{}\"]", rule.driver()),
                format!("\tname = Git Raccoon: {}", rule.tool_name()),
                format!("\tdriver = {}", driver_command(exe, &rule.tool)),
            ],
        });
    }
    out
}

/// Adds `rule`: its lines (a pattern already in a rule of ours is moved to
/// this one), and its merge driver when it's automatic.
pub fn add(path: &str, rule: &Rule, exe: &Path) -> Result<()> {
    if rule.patterns.is_empty() {
        return Err(Error::Unsupported(
            "a rule needs at least one file name".into(),
        ));
    }
    if tools::preset_by_id(&rule.tool).is_none() {
        return Err(Error::Unsupported(format!(
            "\"{}\" isn't a tool Git Raccoon knows",
            rule.tool
        )));
    }
    let repo = Repository::discover(path)?;
    let f = files(&repo)?;
    // The same files in another rule of ours: that one gives them up.
    for file in [&f.mine, &f.shared] {
        rewrite(file, |line| {
            parse_line(line).is_some_and(|(p, _, _)| rule.patterns.contains(&p))
        })?;
    }
    let target = if rule.everyone { &f.shared } else { &f.mine };
    let mut lines = read_lines(target);
    lines.extend(rule.lines());
    write_lines(target, &lines)?;
    if rule.automatic {
        let mut config = repo.config()?.open_level(ConfigLevel::Local)?;
        let d = rule.driver();
        config.set_str(
            &format!("merge.{d}.name"),
            &format!("Git Raccoon: {}", rule.tool_name()),
        )?;
        config.set_str(
            &format!("merge.{d}.driver"),
            &driver_command(exe, &rule.tool),
        )?;
    }
    tidy_drivers(&repo)
}

/// Removes `rule`: exactly its lines, and its driver once nothing uses it.
pub fn remove(path: &str, rule: &Rule) -> Result<()> {
    let repo = Repository::discover(path)?;
    let f = files(&repo)?;
    let target = if rule.everyone { &f.shared } else { &f.mine };
    rewrite(target, |line| {
        parse_line(line).is_some_and(|(p, tool, automatic)| {
            tool == rule.tool && automatic == rule.automatic && rule.patterns.contains(&p)
        })
    })?;
    tidy_drivers(&repo)
}

/// Drops the lines `gone` says to from `file` (left alone if none do).
fn rewrite(file: &Path, gone: impl Fn(&str) -> bool) -> Result<()> {
    let lines = read_lines(file);
    if !lines.iter().any(|l| gone(l)) {
        return Ok(());
    }
    let kept: Vec<String> = lines.into_iter().filter(|l| !gone(l)).collect();
    write_lines(file, &kept)
}

fn write_lines(file: &Path, lines: &[String]) -> Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| Error::Unsupported(format!("can't write {}: {e}", file.display())))?;
    }
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    std::fs::write(file, text)
        .map_err(|e| Error::Unsupported(format!("can't write {}: {e}", file.display())))
}

/// Removes our drivers from `.git/config` that no rule uses any more.
fn tidy_drivers(repo: &Repository) -> Result<()> {
    let path = repo
        .workdir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let used: Vec<String> = rules(&path)?
        .into_iter()
        .filter(|r| r.automatic)
        .map(|r| r.driver())
        .collect();
    let mut config = repo.config()?.open_level(ConfigLevel::Local)?;
    let mut ours: Vec<String> = Vec::new();
    {
        let mut entries = config.entries(Some(&format!("merge\\.{DRIVER_PREFIX}.*")))?;
        while let Some(e) = entries.next() {
            let e = e?;
            if let Some(name) = e.name() {
                ours.push(name.to_string());
            }
        }
    }
    for key in ours {
        let driver = key
            .strip_prefix("merge.")
            .and_then(|k| k.rsplit_once('.'))
            .map(|(d, _)| d.to_string())
            .unwrap_or_default();
        if !used.contains(&driver) {
            let _ = config.remove(&key);
        }
    }
    Ok(())
}

/// The tool a rule gives `file`, if one does (from Git's own reading of the
/// attributes).
pub fn tool_for(path: &str, file: &str) -> Option<String> {
    let repo = Repository::discover(path).ok()?;
    let value = repo
        .get_attr(
            Path::new(file),
            TOOL_ATTR,
            git2::AttrCheckFlags::FILE_THEN_INDEX,
        )
        .ok()??;
    Some(value.to_string()).filter(|v| tools::preset_by_id(v).is_some())
}

/// What the merge driver did with a file, as kept in the repository's
/// `.git/raccoon-merges` (newest last) for the app to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverResult {
    /// Seconds since the Unix epoch.
    pub at: i64,
    pub file: String,
    pub tool: String,
    pub merged: bool,
    /// Why it's left as a conflict.
    pub why: String,
}

const LOG: &str = "raccoon-merges";

/// What the merge driver has done since `since` (seconds).
pub fn driver_results(path: &str, since: i64) -> Vec<DriverResult> {
    let Ok(repo) = Repository::discover(path) else {
        return Vec::new();
    };
    read_lines(&repo.commondir().join(LOG))
        .iter()
        .filter_map(|l| {
            let mut f = l.splitn(5, '\t');
            Some(DriverResult {
                at: f.next()?.parse().ok()?,
                file: f.next()?.to_string(),
                tool: f.next()?.to_string(),
                merged: f.next()? == "merged",
                why: f.next().unwrap_or_default().to_string(),
            })
        })
        .filter(|r| r.at >= since)
        .collect()
}

/// What the merge driver did in what's in progress now (since it started:
/// Git notes where it began in ORIG_HEAD), the latest for each file.
pub fn driver_results_now(path: &str) -> Vec<DriverResult> {
    let Ok(repo) = Repository::discover(path) else {
        return Vec::new();
    };
    let started = std::fs::metadata(repo.path().join("ORIG_HEAD"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0);
    let mut out: Vec<DriverResult> = Vec::new();
    for r in driver_results(path, started - 2) {
        out.retain(|o| o.file != r.file);
        out.push(r);
    }
    out
}

fn note(git_dir: &Path, r: &DriverResult) {
    use std::io::Write;
    let path = git_dir.join(LOG);
    // Only the latest few hundred are kept.
    let mut lines = read_lines(&path);
    if lines.len() > 400 {
        lines.drain(..lines.len() - 200);
        let _ = write_lines(&path, &lines);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let state = if r.merged { "merged" } else { "conflict" };
        let why = r.why.replace(['\t', '\n'], " ");
        let _ = writeln!(f, "{}\t{}\t{}\t{state}\t{why}", r.at, r.file, r.tool);
    }
}

/// `git-raccoon merge-driver <tool> <base> <ours> <theirs> <path>`, run by
/// Git during a merge: the tool merges the file into `ours`. When it can't
/// (or isn't installed), `ours` is left as Git's usual conflict, never
/// half-merged. Returns the exit code for Git: 0 merged, 1 a conflict.
pub fn run_driver(args: &[String]) -> i32 {
    run_driver_in(Path::new("."), args)
}

/// The driver, run in `dir` (where Git starts it: the top of the tree).
fn run_driver_in(dir: &Path, args: &[String]) -> i32 {
    let [tool, base, ours, theirs, file] = args else {
        eprintln!("usage: git-raccoon merge-driver <tool> %O %A %B %P");
        return 2;
    };
    let repo = Repository::discover(dir).ok();
    let root = repo
        .as_ref()
        .and_then(|r| r.workdir().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let git_dir = repo.as_ref().map(|r| r.commondir().to_path_buf());
    let before = std::fs::read(ours).ok();
    let (merged, why) = match drive(tool, base, ours, theirs, file, &root) {
        Ok(()) => (true, String::new()),
        Err(why) => (false, why),
    };
    if !merged {
        // Back to ours, then Git's usual conflict markers.
        if let Some(b) = &before {
            let _ = std::fs::write(ours, b);
        }
        let _ = std::process::Command::new("git")
            .args([
                "merge-file",
                "-L",
                "ours",
                "-L",
                "base",
                "-L",
                "theirs",
                ours,
                base,
                theirs,
            ])
            .status();
    }
    if let Some(dir) = git_dir {
        note(
            &dir,
            &DriverResult {
                at: chrono::Utc::now().timestamp(),
                file: file.clone(),
                tool: tool.clone(),
                merged,
                why,
            },
        );
    }
    if merged { 0 } else { 1 }
}

fn drive(
    tool: &str,
    base: &str,
    ours: &str,
    theirs: &str,
    file: &str,
    root: &Path,
) -> std::result::Result<(), String> {
    let preset = tools::preset_by_id(tool)
        .ok_or_else(|| format!("\"{tool}\" isn't a tool Git Raccoon knows"))?;
    let args = preset
        .driver
        .ok_or_else(|| format!("{} can't merge on its own", preset.name))?;
    let places = Places::here();
    let program = if preset.id == tools::UNITY {
        let project = tools::unity_project(root, file).unwrap_or_else(|| root.to_path_buf());
        tools::unity_for(&project, &places)?.yaml_merge
    } else {
        tools::find(preset, &places)
            .ok_or_else(|| format!("{} wasn't found", preset.name))?
            .program
    };
    let vars = tools::Vars {
        base: base.into(),
        local: ours.into(),
        remote: theirs.into(),
        merged: ours.into(),
        file: file.into(),
        line: 1,
    };
    let status = std::process::Command::new(&program)
        .args(args.iter().map(|a| vars.fill(a)))
        .stdin(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("{} didn't start: {e}", preset.name))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} couldn't merge it ({})",
            preset.name,
            status
                .code()
                .map_or("stopped".into(), |c| format!("exit {c}"))
        ))
    }
}

/// Runs a rule's tool on a copy of `sample` merged with itself, to see it
/// works here: what it said.
pub fn test_on(path: &str, rule: &Rule, sample: &str) -> std::result::Result<String, String> {
    let repo = Repository::discover(path).map_err(|e| e.to_string())?;
    let root = repo
        .workdir()
        .ok_or("a bare repository has no files")?
        .to_path_buf();
    let preset = tools::preset_by_id(&rule.tool).ok_or("not a tool Git Raccoon knows")?;
    let bytes =
        std::fs::read(root.join(sample)).map_err(|e| format!("can't read {sample}: {e}"))?;
    let dir = std::env::temp_dir().join(format!("git-raccoon-rule-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let paths: Vec<String> = ["base", "ours", "theirs"]
        .iter()
        .map(|n| {
            let p = dir.join(n);
            let _ = std::fs::write(&p, &bytes);
            p.display().to_string()
        })
        .collect();
    let result = if preset.can_drive() {
        drive(&rule.tool, &paths[0], &paths[1], &paths[2], sample, &root).and_then(|()| {
            match std::fs::read(&paths[1]) {
                Ok(after) if after == bytes => Ok(format!(
                    "{} merged {sample} with itself, and left it as it was: it works here.",
                    preset.name
                )),
                Ok(_) => Ok(format!(
                    "{} merged {sample} with itself (its result differs a little, as some tools rewrite files).",
                    preset.name
                )),
                Err(e) => Err(e.to_string()),
            }
        })
    } else {
        tools::find(preset, &Places::here())
            .map(|f| format!("{} is at {}.", preset.name, f.program.display()))
            .ok_or_else(|| format!("{} wasn't found on this computer", preset.name))
    };
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Files in the repository that `patterns` match, for picking a sample.
pub fn matching(path: &str, patterns: &[String], limit: usize) -> Vec<String> {
    let Ok(repo) = Repository::discover(path) else {
        return Vec::new();
    };
    let Ok(index) = repo.index() else {
        return Vec::new();
    };
    index
        .iter()
        .map(|e| String::from_utf8_lossy(&e.path).to_string())
        .filter(|f| patterns.iter().any(|p| glob(p, f)))
        .take(limit)
        .collect()
}

/// Whether `file` matches an attributes pattern: one without a slash
/// matches the name in any folder; `*` doesn't cross folders, `**` does.
pub fn glob(pattern: &str, file: &str) -> bool {
    let pattern = pattern.trim_start_matches('/');
    if !pattern.contains('/') {
        let name = file.rsplit('/').next().unwrap_or(file);
        return wild(pattern.as_bytes(), name.as_bytes());
    }
    wild(pattern.as_bytes(), file.as_bytes())
}

fn wild(p: &[u8], s: &[u8]) -> bool {
    match (p.first(), s.first()) {
        (None, None) => true,
        (Some(b'*'), _) if p.get(1) == Some(&b'*') => {
            let rest = p[2..].strip_prefix(b"/").unwrap_or(&p[2..]);
            (0..=s.len()).any(|i| wild(rest, &s[i..]))
        }
        (Some(b'*'), _) => (0..=s.len())
            .take_while(|&i| i == 0 || s[i - 1] != b'/')
            .any(|i| wild(&p[1..], &s[i..])),
        (Some(b'?'), Some(c)) if *c != b'/' => wild(&p[1..], &s[1..]),
        (Some(a), Some(b)) if a == b => wild(&p[1..], &s[1..]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    fn unity_rule(everyone: bool) -> Rule {
        Rule {
            patterns: vec!["*.unity".into(), "*.prefab".into()],
            tool: tools::UNITY.into(),
            automatic: true,
            everyone,
        }
    }

    #[test]
    fn writes_rules_as_gits_attributes_and_reads_them_back() {
        let t = TempRepo::new();
        let exe = Path::new("/opt/Git Raccoon/git-raccoon");
        let rule = unity_rule(false);
        let changes = preview(&rule, exe);
        assert_eq!(changes[0].file, ".git/info/attributes");
        assert_eq!(
            changes[0].lines[0],
            "*.unity merge=raccoon-unityyamlmerge raccoon-tool=unityyamlmerge"
        );
        assert!(
            changes[1].lines[2].contains(
                "\"/opt/Git Raccoon/git-raccoon\" merge-driver unityyamlmerge %O %A %B %P"
            )
        );
        // Nothing's written by a preview.
        assert!(rules(t.path()).unwrap().is_empty());

        add(t.path(), &rule, exe).unwrap();
        assert_eq!(rules(t.path()).unwrap(), std::slice::from_ref(&rule));
        let config = t.repo.config().unwrap().snapshot().unwrap();
        assert!(
            config
                .get_string("merge.raccoon-unityyamlmerge.driver")
                .unwrap()
                .contains("merge-driver")
        );
        // Git sees it.
        assert_eq!(
            tool_for(t.path(), "Assets/Main.unity").as_deref(),
            Some(tools::UNITY)
        );
        assert_eq!(
            crate::git_output(
                t.path(),
                &["check-attr", "merge", "--", "Assets/Main.unity"]
            )
            .unwrap()
            .trim(),
            "Assets/Main.unity: merge: raccoon-unityyamlmerge"
        );
        assert!(tool_for(t.path(), "README.md").is_none());

        // One for opening only, for everyone: in .gitattributes, no driver.
        let open = Rule {
            patterns: vec!["*.psd".into()],
            tool: "p4merge".into(),
            automatic: false,
            everyone: true,
        };
        add(t.path(), &open, exe).unwrap();
        assert_eq!(rules(t.path()).unwrap(), [rule.clone(), open.clone()]);
        assert_eq!(
            std::fs::read_to_string(t.dir.join(".gitattributes")).unwrap(),
            "*.psd raccoon-tool=p4merge\n"
        );

        // A pattern moves to the newest rule that names it.
        let moved = Rule {
            patterns: vec!["*.prefab".into()],
            tool: "meld".into(),
            automatic: false,
            everyone: false,
        };
        add(t.path(), &moved, exe).unwrap();
        let now = rules(t.path()).unwrap();
        assert_eq!(now[0].patterns, ["*.unity"]);

        // Removing takes exactly its lines, and the driver with the last.
        let unity_now = now[0].clone();
        remove(t.path(), &unity_now).unwrap();
        let config = t.repo.config().unwrap().snapshot().unwrap();
        assert!(
            config
                .get_string("merge.raccoon-unityyamlmerge.driver")
                .is_err()
        );
        assert_eq!(rules(t.path()).unwrap(), [moved, open]);
    }

    #[test]
    fn keeps_other_lines_in_the_attributes_files() {
        let t = TempRepo::new();
        t.write(".gitattributes", "*.png binary\n# ours\n");
        let rule = Rule {
            patterns: vec!["*.asset".into()],
            tool: tools::UNITY.into(),
            automatic: false,
            everyone: true,
        };
        add(t.path(), &rule, Path::new("gr")).unwrap();
        remove(t.path(), &rule).unwrap();
        assert_eq!(
            std::fs::read_to_string(t.dir.join(".gitattributes")).unwrap(),
            "*.png binary\n# ours\n"
        );
    }

    #[test]
    fn matches_file_names_like_git() {
        assert!(glob("*.unity", "Assets/Scenes/Main.unity"));
        assert!(!glob("*.unity", "Assets/Main.unity.meta"));
        assert!(glob("Assets/**/*.prefab", "Assets/A/B/x.prefab"));
        assert!(glob("Assets/*.prefab", "Assets/x.prefab"));
        assert!(!glob("Assets/*.prefab", "Assets/A/x.prefab"));
        assert!(glob("/README.md", "README.md"));
        assert_eq!(
            split_patterns(" *.unity  *.prefab\n*.meta "),
            ["*.unity", "*.prefab", "*.meta"]
        );
    }

    #[test]
    fn a_driver_that_cant_merge_leaves_gits_usual_conflict() {
        let t = TempRepo::new();
        let dir = &t.dir;
        let write = |n: &str, s: &str| {
            std::fs::write(dir.join(n), s).unwrap();
            dir.join(n).display().to_string()
        };
        let base = write("b", "one\ntwo\nthree\n");
        let ours = write("o", "one\nTWO\nthree\n");
        let theirs = write("t", "one\nzwei\nthree\n");
        // Not a tool that can merge on its own (and not here anyway).
        let code = run_driver_in(
            dir,
            &[
                "meld".into(),
                base,
                ours.clone(),
                theirs,
                "Assets/x.txt".into(),
            ],
        );
        assert_eq!(code, 1);
        let text = std::fs::read_to_string(&ours).unwrap();
        assert!(
            text.contains("<<<<<<< ours") && text.contains("TWO") && text.contains("zwei"),
            "{text}"
        );
        let log = driver_results(t.path(), 0);
        assert_eq!(log.len(), 1);
        assert!(!log[0].merged && log[0].file == "Assets/x.txt");
        assert!(
            log[0].why.contains("can't merge on its own"),
            "{}",
            log[0].why
        );
    }
}
