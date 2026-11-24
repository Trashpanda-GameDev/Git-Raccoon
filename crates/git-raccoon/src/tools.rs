//! Which merge tool, diff tool and editor to use, from Settings: what's
//! found on this computer (looked for once), and the command to start.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use git_raccoon_core::tools::{self, Found, Places, Role, Run, Vars};

use crate::settings::{Settings, ToolChoice};

static FOUND: Mutex<Option<Arc<BTreeMap<&'static str, Found>>>> = Mutex::new(None);
static THIS_PROGRAM: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

/// Where to look for tools instead of this computer's usual places, and
/// the program Git is to run as a merge driver (tests).
pub fn look_in(places: Places) {
    tools::look_in(Some(places));
    forget();
}

pub fn run_as_driver(program: std::path::PathBuf) {
    if let Ok(mut p) = THIS_PROGRAM.lock() {
        *p = Some(program);
    }
}

fn places() -> Places {
    Places::here()
}

/// The tools found here, by id (looked for the first time it's asked).
pub fn found() -> Arc<BTreeMap<&'static str, Found>> {
    let mut f = FOUND.lock().unwrap_or_else(|e| e.into_inner());
    Arc::clone(f.get_or_insert_with(|| Arc::new(tools::found_by_id(&places()))))
}

/// Looks again next time (something was installed).
pub fn forget() {
    if let Ok(mut f) = FOUND.lock() {
        *f = None;
    }
}

/// The tools found here that can do `role`, in the presets' order.
pub fn found_for(role: Role) -> Vec<Found> {
    let found = found();
    tools::PRESETS
        .iter()
        .filter(|p| p.does(role))
        .filter_map(|p| found.get(p.id).cloned())
        .collect()
}

pub fn choice(settings: &Settings, role: Role) -> &ToolChoice {
    match role {
        Role::Merge => &settings.merge_tool,
        Role::Diff => &settings.diff_tool,
        Role::Editor => &settings.editor,
    }
}

pub fn choice_mut(settings: &mut Settings, role: Role) -> &mut ToolChoice {
    match role {
        Role::Merge => &mut settings.merge_tool,
        Role::Diff => &mut settings.diff_tool,
        Role::Editor => &mut settings.editor,
    }
}

/// What a choice is called, for a list: "Git config default (KDiff3)",
/// "Use the merge tool", "Beyond Compare".
pub fn label(choice: &ToolChoice, role: Role, repo: Option<&str>) -> String {
    match choice {
        ToolChoice::Default => match role {
            Role::Merge => match tools::git_tool(repo, Role::Merge) {
                Some(t) => format!("Git config default ({})", t.label()),
                None => "Git config default (none set)".into(),
            },
            Role::Diff => "Use the merge tool".into(),
            Role::Editor => "None".into(),
        },
        ToolChoice::BuiltIn => "Built in only".into(),
        ToolChoice::Preset(id) => tools::preset_by_id(id).map_or_else(
            || id.clone(),
            |p| {
                if found().contains_key(p.id) {
                    p.name.to_string()
                } else {
                    format!("{} (not found)", p.name)
                }
            },
        ),
        ToolChoice::Custom(_) => "Custom command…".into(),
    }
}

/// The tool to use for `role`: its name and how to start it, or `None` for
/// Git Raccoon's own view. `Err` says why the one chosen can't be started.
pub fn resolve(
    settings: &Settings,
    role: Role,
    repo: Option<&str>,
    vars: &Vars,
) -> Result<Option<(String, Run)>, String> {
    let mut choice = choice(settings, role).clone();
    // A diff tool that's "the merge tool": its choice, used for a diff
    // (Git's own diff.tool when that's Git's).
    if role == Role::Diff && choice == ToolChoice::Default {
        choice = match &settings.merge_tool {
            // A merge command of one's own takes a merge's four files, not
            // a diff's two: the built-in view.
            ToolChoice::Custom(_) => ToolChoice::BuiltIn,
            other => other.clone(),
        };
    }
    let as_role = role;
    match choice {
        ToolChoice::BuiltIn => Ok(None),
        ToolChoice::Default if role == Role::Editor => Ok(None),
        ToolChoice::Default => {
            let Some(git) = tools::git_tool(repo, as_role) else {
                return Ok(None);
            };
            let run = tools::git_tool_run(&git, as_role, vars, &places())?;
            Ok(Some((git.label(), run)))
        }
        ToolChoice::Preset(id) => {
            let preset = tools::preset_by_id(&id).ok_or_else(|| format!("\"{id}\" isn't known"))?;
            let found = found()
                .get(preset.id)
                .cloned()
                .ok_or_else(|| format!("{} wasn't found on this computer", preset.name))?;
            let run = found
                .run(as_role, vars)
                .ok_or_else(|| format!("{} can't do that", preset.name))?;
            Ok(Some((preset.name.to_string(), run)))
        }
        ToolChoice::Custom(line) => {
            let run = tools::custom(&line, vars).ok_or("the custom command is empty")?;
            let name = run
                .program
                .file_stem()
                .map_or_else(|| "the tool".into(), |s| s.to_string_lossy().to_string());
            Ok(Some((name, run)))
        }
    }
}

/// Whether there's a tool (not the built-in view) for `role`.
pub fn has(settings: &Settings, role: Role, repo: Option<&str>) -> bool {
    matches!(resolve(settings, role, repo, &Vars::default()), Ok(Some(_)))
}

/// The name of the tool `role` would use (`None`: the built-in view, or
/// one that can't be started). Remembered, as it reads Git's config.
pub fn name_for(settings: &Settings, role: Role, repo: Option<&str>) -> Option<String> {
    type Key = (Role, ToolChoice, ToolChoice, Option<String>);
    static NAMES: Mutex<Vec<(Key, Option<String>)>> = Mutex::new(Vec::new());
    let key: Key = (
        role,
        choice(settings, role).clone(),
        settings.merge_tool.clone(),
        repo.map(str::to_string),
    );
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, name)) = names.iter().find(|(k, _)| *k == key) {
        return name.clone();
    }
    let name = resolve(settings, role, repo, &Vars::default())
        .ok()
        .flatten()
        .map(|(n, _)| n);
    if names.len() > 32 {
        names.clear();
    }
    names.push((key, name.clone()));
    name
}

/// The merge tool's command for a conflict's versions: the one chosen in
/// Settings, or `pick` (a tool found here, by id) instead.
pub fn merge_run(
    settings: &Settings,
    repo: &str,
    pick: Option<&str>,
    v: &git_raccoon_core::conflict::Versions,
) -> Result<(String, Run), String> {
    let vars = Vars {
        base: v.base.display().to_string(),
        local: v.ours.display().to_string(),
        remote: v.theirs.display().to_string(),
        merged: v.merged.display().to_string(),
        file: v.merged.display().to_string(),
        line: 1,
    };
    // A rule for this file names its tool, unless another was picked.
    let file = v
        .merged
        .strip_prefix(repo)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let ruled = git_raccoon_core::rules::tool_for(repo, &file);
    let pick = pick.map(str::to_string).or(ruled);
    match pick.as_deref() {
        Some(id) => {
            let found = if id == tools::UNITY {
                // UnityYAMLMerge from the project's own Unity.
                let root = std::path::Path::new(repo);
                let project =
                    tools::unity_project(root, &file).unwrap_or_else(|| root.to_path_buf());
                let editor = tools::unity_for(&project, &places())?;
                Found {
                    preset: tools::preset_by_id(tools::UNITY)
                        .ok_or("UnityYAMLMerge isn't known")?,
                    program: editor.yaml_merge,
                    version: Some(editor.version),
                }
            } else {
                found()
                    .get(id)
                    .cloned()
                    .ok_or_else(|| format!("{id} wasn't found on this computer"))?
            };
            let run = found
                .run(Role::Merge, &vars)
                .ok_or_else(|| format!("{} can't merge", found.preset.name))?;
            Ok((found.preset.name.to_string(), run))
        }
        None => resolve(settings, Role::Merge, Some(repo), &vars)?
            .ok_or_else(|| "No merge tool is chosen: pick one in Settings › Tools".to_string()),
    }
}

/// Brings a program's window to the front, as far as the system allows.
pub fn bring_forward(pid: u32) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let mut c = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args([
            "-NoProfile",
            "-Command",
            &format!("(New-Object -ComObject WScript.Shell).AppActivate({pid}) | Out-Null"),
        ]);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = Command::new("osascript");
        c.args([
            "-e",
            &format!(
                "tell application \"System Events\" to set frontmost of (first process whose unix id is {pid}) to true"
            ),
        ]);
        c
    } else {
        let mut c = Command::new("xdotool");
        c.args([
            "search",
            "--pid",
            &pid.to_string(),
            "--onlyvisible",
            "windowactivate",
        ]);
        c
    };
    c.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("couldn't bring it forward ({e})"))
}

/// This program, for Git to run as a merge driver: the AppImage when run
/// from one (its own path changes each run), or this executable.
pub fn this_program() -> std::path::PathBuf {
    if let Some(p) = THIS_PROGRAM.lock().ok().and_then(|p| p.clone()) {
        return p;
    }
    std::env::var_os("APPIMAGE")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_else(|| "git-raccoon".into())
}
