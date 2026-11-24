//! Which merge tool, diff tool and editor to use, from Settings: what's
//! found on this computer (looked for once), and the command to start.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use git_raccoon_core::tools::{self, Found, Places, Role, Run, Vars};

use crate::settings::{Settings, ToolChoice};

static FOUND: Mutex<Option<Arc<BTreeMap<&'static str, Found>>>> = Mutex::new(None);
static PLACES: Mutex<Option<Places>> = Mutex::new(None);

/// Where to look for tools instead of this computer's usual places (tests).
pub fn look_in(places: Places) {
    if let Ok(mut p) = PLACES.lock() {
        *p = Some(places);
    }
    forget();
}

fn places() -> Places {
    PLACES
        .lock()
        .ok()
        .and_then(|p| p.clone())
        .unwrap_or_else(Places::here)
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
