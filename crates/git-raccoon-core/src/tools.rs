//! Other programs to look at changes with: merge tools, diff tools and
//! editors. The ones Git Raccoon knows (presets), finding them on this
//! computer, the command lines that start them, and Git's own
//! `merge.tool` / `diff.tool` settings.
//!
//! Commands are lists of words, each its own argument (no shell), with
//! placeholders filled in whole: `$BASE $LOCAL $REMOTE $MERGED` for a merge,
//! `$LOCAL $REMOTE` for a diff, `$FILE $LINE` for an editor. Only a command
//! from Git's config (`mergetool.<name>.cmd`) goes through a shell, as Git
//! runs it, with the placeholders as variables.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What a tool is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Merge,
    Diff,
    Editor,
}

/// Which system's places to look in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Windows,
    Mac,
    Linux,
}

impl System {
    pub fn this() -> System {
        if cfg!(windows) {
            System::Windows
        } else if cfg!(target_os = "macos") {
            System::Mac
        } else {
            System::Linux
        }
    }
}

/// A program Git Raccoon knows how to start.
#[derive(Debug)]
pub struct Preset {
    /// Its id in settings, and Git's name for it where Git has one.
    pub id: &'static str,
    pub name: &'static str,
    /// Its arguments (after the program) for each role it can do.
    pub merge: Option<&'static [&'static str]>,
    pub diff: Option<&'static [&'static str]>,
    pub edit: Option<&'static [&'static str]>,
    /// Its program's names on the PATH (".exe" is added on Windows).
    commands: &'static [&'static str],
    /// Where it's usually installed: relative to Program Files (both),
    /// `%LOCALAPPDATA%`, `/Applications`, and the home folder.
    windows: &'static [&'static str],
    windows_local: &'static [&'static str],
    mac: &'static [&'static str],
    home: &'static [&'static str],
    /// Git's other names for it (`merge.tool = bc4`).
    git_names: &'static [&'static str],
}

impl Preset {
    pub fn does(&self, role: Role) -> bool {
        self.args(role).is_some()
    }

    fn args(&self, role: Role) -> Option<&'static [&'static str]> {
        match role {
            Role::Merge => self.merge,
            Role::Diff => self.diff,
            Role::Editor => self.edit,
        }
    }

    /// Whether this is the one Git calls `name`.
    pub fn is_called(&self, name: &str) -> bool {
        self.id == name || self.git_names.contains(&name)
    }
}

const fn preset(id: &'static str, name: &'static str) -> Preset {
    Preset {
        id,
        name,
        merge: None,
        diff: None,
        edit: None,
        commands: &[],
        windows: &[],
        windows_local: &[],
        mac: &[],
        home: &[],
        git_names: &[],
    }
}

/// A JetBrains IDE: `merge`, `diff` and `--line` are the same for all.
const fn jetbrains(
    id: &'static str,
    name: &'static str,
    commands: &'static [&'static str],
    windows: &'static [&'static str],
    mac: &'static [&'static str],
) -> Preset {
    Preset {
        merge: Some(&["merge", "$LOCAL", "$REMOTE", "$BASE", "$MERGED"]),
        diff: Some(&["diff", "$LOCAL", "$REMOTE"]),
        edit: Some(&["--line", "$LINE", "$FILE"]),
        commands,
        windows,
        mac,
        // JetBrains Toolbox's shell scripts.
        home: &[
            ".local/share/JetBrains/Toolbox/scripts",
            "Library/Application Support/JetBrains/Toolbox/scripts",
        ],
        windows_local: &[r"JetBrains\Toolbox\scripts"],
        ..preset(id, name)
    }
}

/// Every tool Git Raccoon knows, in the order they're listed.
pub static PRESETS: &[Preset] = &[
    Preset {
        merge: Some(&[
            "--L1", "Base", "--L2", "Ours", "--L3", "Theirs", "-o", "$MERGED", "$BASE", "$LOCAL",
            "$REMOTE",
        ]),
        diff: Some(&["$LOCAL", "$REMOTE"]),
        commands: &["kdiff3"],
        windows: &[r"KDiff3\kdiff3.exe", r"KDiff3\bin\kdiff3.exe"],
        mac: &["kdiff3.app/Contents/MacOS/kdiff3"],
        ..preset("kdiff3", "KDiff3")
    },
    Preset {
        merge: Some(&["--output", "$MERGED", "$LOCAL", "$BASE", "$REMOTE"]),
        diff: Some(&["$LOCAL", "$REMOTE"]),
        commands: &["meld"],
        windows: &[r"Meld\Meld.exe"],
        windows_local: &[r"Programs\Meld\Meld.exe"],
        mac: &["Meld.app/Contents/MacOS/Meld"],
        ..preset("meld", "Meld")
    },
    Preset {
        merge: Some(&["$LOCAL", "$REMOTE", "$BASE", "-mergeoutput=$MERGED"]),
        diff: Some(&["$LOCAL", "$REMOTE"]),
        commands: &["bcompare", "bcomp"],
        windows: &[
            r"Beyond Compare 5\BComp.exe",
            r"Beyond Compare 4\BComp.exe",
            r"Beyond Compare 3\BComp.exe",
        ],
        mac: &["Beyond Compare.app/Contents/MacOS/bcomp"],
        git_names: &["bc", "bc3", "bc4", "bc5"],
        ..preset("bcompare", "Beyond Compare")
    },
    Preset {
        merge: Some(&["$BASE", "$LOCAL", "$REMOTE", "$MERGED"]),
        diff: Some(&["$LOCAL", "$REMOTE"]),
        commands: &["p4merge"],
        windows: &[r"Perforce\p4merge.exe"],
        mac: &["p4merge.app/Contents/Resources/launchp4merge"],
        ..preset("p4merge", "P4Merge")
    },
    Preset {
        merge: Some(&[
            "-u", "-e", "-wl", "-wr", "-fm", "-dl", "Ours", "-dm", "Base", "-dr", "Theirs",
            "$LOCAL", "$BASE", "$REMOTE", "-o", "$MERGED",
        ]),
        diff: Some(&["-u", "-e", "$LOCAL", "$REMOTE"]),
        commands: &["WinMergeU"],
        windows: &[r"WinMerge\WinMergeU.exe"],
        windows_local: &[r"Programs\WinMerge\WinMergeU.exe"],
        ..preset("winmerge", "WinMerge")
    },
    Preset {
        merge: Some(&["--wait", "--merge", "$REMOTE", "$LOCAL", "$BASE", "$MERGED"]),
        diff: Some(&["--wait", "--diff", "$LOCAL", "$REMOTE"]),
        edit: Some(&["--goto", "$FILE:$LINE"]),
        commands: &["code"],
        windows: &[r"Microsoft VS Code\bin\code.cmd"],
        windows_local: &[r"Programs\Microsoft VS Code\bin\code.cmd"],
        mac: &["Visual Studio Code.app/Contents/Resources/app/bin/code"],
        git_names: &["vscode"],
        ..preset("code", "Visual Studio Code")
    },
    jetbrains(
        "rider",
        "Rider",
        &["rider", "rider64"],
        &[r"JetBrains\JetBrains Rider\bin\rider64.exe"],
        &["Rider.app/Contents/MacOS/rider"],
    ),
    jetbrains(
        "idea",
        "IntelliJ IDEA",
        &["idea", "idea64"],
        &[
            r"JetBrains\IntelliJ IDEA\bin\idea64.exe",
            r"JetBrains\IntelliJ IDEA Community Edition\bin\idea64.exe",
        ],
        &[
            "IntelliJ IDEA.app/Contents/MacOS/idea",
            "IntelliJ IDEA CE.app/Contents/MacOS/idea",
        ],
    ),
    jetbrains(
        "webstorm",
        "WebStorm",
        &["webstorm", "webstorm64"],
        &[r"JetBrains\WebStorm\bin\webstorm64.exe"],
        &["WebStorm.app/Contents/MacOS/webstorm"],
    ),
    jetbrains(
        "pycharm",
        "PyCharm",
        &["pycharm", "pycharm64"],
        &[
            r"JetBrains\PyCharm\bin\pycharm64.exe",
            r"JetBrains\PyCharm Community Edition\bin\pycharm64.exe",
        ],
        &[
            "PyCharm.app/Contents/MacOS/pycharm",
            "PyCharm CE.app/Contents/MacOS/pycharm",
        ],
    ),
    jetbrains(
        "clion",
        "CLion",
        &["clion", "clion64"],
        &[r"JetBrains\CLion\bin\clion64.exe"],
        &["CLion.app/Contents/MacOS/clion"],
    ),
    jetbrains(
        "goland",
        "GoLand",
        &["goland", "goland64"],
        &[r"JetBrains\GoLand\bin\goland64.exe"],
        &["GoLand.app/Contents/MacOS/goland"],
    ),
    jetbrains(
        "rustrover",
        "RustRover",
        &["rustrover", "rustrover64"],
        &[r"JetBrains\RustRover\bin\rustrover64.exe"],
        &["RustRover.app/Contents/MacOS/rustrover"],
    ),
    Preset {
        // Unity's own: merges scenes, prefabs and other YAML assets.
        merge: Some(&["merge", "-p", "$BASE", "$REMOTE", "$LOCAL", "$MERGED"]),
        ..preset(UNITY, "UnityYAMLMerge")
    },
    Preset {
        edit: Some(&["$FILE:$LINE"]),
        commands: &["subl"],
        windows: &[r"Sublime Text\subl.exe", r"Sublime Text 3\subl.exe"],
        mac: &["Sublime Text.app/Contents/SharedSupport/bin/subl"],
        ..preset("sublime", "Sublime Text")
    },
    Preset {
        edit: Some(&["-n$LINE", "$FILE"]),
        commands: &["notepad++"],
        windows: &[r"Notepad++\notepad++.exe"],
        ..preset("notepad++", "Notepad++")
    },
    Preset {
        edit: Some(&["$FILE:$LINE"]),
        commands: &["zed", "zeditor"],
        windows_local: &[r"Programs\Zed\bin\zed.exe"],
        mac: &["Zed.app/Contents/MacOS/cli"],
        home: &[".local/bin/zed"],
        ..preset("zed", "Zed")
    },
];

/// UnityYAMLMerge's id: it comes with each Unity editor, so it's found
/// through them rather than on the PATH.
pub const UNITY: &str = "unityyamlmerge";

impl PartialEq for Found {
    fn eq(&self, other: &Self) -> bool {
        self.preset.id == other.preset.id
            && self.program == other.program
            && self.version == other.version
    }
}

pub fn preset_by_id(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// Where to look: the PATH and the system's usual folders. Built from the
/// environment, or by tests from a folder of fake installs.
#[derive(Debug, Clone, Default)]
pub struct Places {
    pub system: Option<System>,
    pub path: Vec<PathBuf>,
    pub home: Option<PathBuf>,
    /// `C:\Program Files` and `C:\Program Files (x86)`.
    pub program_files: Vec<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub app_data: Option<PathBuf>,
    pub applications: Vec<PathBuf>,
}

impl Places {
    pub fn here() -> Places {
        let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
        let system = System::this();
        let home = var("HOME").or_else(|| var("USERPROFILE"));
        let mut path: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        // Places apps put programs that a desktop session's PATH can miss.
        if system != System::Windows {
            for extra in ["/usr/local/bin", "/opt/homebrew/bin", "/snap/bin"] {
                path.push(PathBuf::from(extra));
            }
            if let Some(h) = &home {
                path.push(h.join(".local/bin"));
            }
        }
        let program_files = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
            .iter()
            .filter_map(|k| var(k))
            .collect();
        let mut applications = vec![PathBuf::from("/Applications")];
        if let Some(h) = &home {
            applications.push(h.join("Applications"));
        }
        Places {
            system: Some(system),
            path,
            home,
            program_files,
            local_app_data: var("LOCALAPPDATA"),
            app_data: var("APPDATA"),
            applications,
        }
    }

    fn system(&self) -> System {
        self.system.unwrap_or_else(System::this)
    }

    /// A program on the PATH.
    pub fn on_path(&self, command: &str) -> Option<PathBuf> {
        let names: Vec<String> = if self.system() == System::Windows {
            ["exe", "cmd", "bat"]
                .iter()
                .map(|ext| format!("{command}.{ext}"))
                .collect()
        } else {
            vec![command.to_string()]
        };
        self.path
            .iter()
            .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
            .find(|p| is_program(p))
    }
}

fn is_program(p: &Path) -> bool {
    p.is_file()
}

/// A tool found on this computer.
#[derive(Debug, Clone)]
pub struct Found {
    pub preset: &'static Preset,
    pub program: PathBuf,
    /// For UnityYAMLMerge: the Unity version it came with.
    pub version: Option<String>,
}

/// Where `preset` is installed, if it is.
pub fn find(preset: &'static Preset, places: &Places) -> Option<Found> {
    if preset.id == UNITY {
        return unity_editors(places).into_iter().next().map(|e| Found {
            preset,
            program: e.yaml_merge,
            version: Some(e.version),
        });
    }
    let found = |program: PathBuf| Found {
        preset,
        program,
        version: None,
    };
    let system = places.system();
    let mut usual: Vec<PathBuf> = Vec::new();
    match system {
        System::Windows => {
            for base in &places.program_files {
                usual.extend(preset.windows.iter().map(|p| base.join(p)));
            }
            if let Some(local) = &places.local_app_data {
                usual.extend(preset.windows_local.iter().map(|p| local.join(p)));
                for scripts in preset
                    .windows_local
                    .iter()
                    .filter(|p| p.ends_with("scripts"))
                {
                    for c in preset.commands {
                        usual.push(local.join(scripts).join(format!("{c}.cmd")));
                    }
                }
            }
        }
        System::Mac => {
            for apps in &places.applications {
                usual.extend(preset.mac.iter().map(|p| apps.join(p)));
            }
        }
        System::Linux => {}
    }
    if system != System::Windows
        && let Some(home) = &places.home
    {
        for p in preset.home {
            let at = home.join(p);
            if at.is_dir() {
                usual.extend(preset.commands.iter().map(|c| at.join(c)));
            } else {
                usual.push(at);
            }
        }
    }
    if let Some(p) = usual.into_iter().find(|p| is_program(p)) {
        return Some(found(p));
    }
    preset
        .commands
        .iter()
        .find_map(|c| places.on_path(c))
        .map(found)
}

/// Every known tool found here that can do `role`, in the presets' order.
pub fn find_all(role: Role, places: &Places) -> Vec<Found> {
    PRESETS
        .iter()
        .filter(|p| p.does(role))
        .filter_map(|p| find(p, places))
        .collect()
}

/// A Unity editor installed through Unity Hub (or where it puts them).
#[derive(Debug, Clone, PartialEq)]
pub struct UnityEditor {
    pub version: String,
    pub yaml_merge: PathBuf,
}

/// Unity editors installed here, newest first.
pub fn unity_editors(places: &Places) -> Vec<UnityEditor> {
    let system = places.system();
    let mut roots: Vec<PathBuf> = Vec::new();
    match system {
        System::Windows => {
            roots.extend(
                places
                    .program_files
                    .iter()
                    .map(|p| p.join(r"Unity\Hub\Editor")),
            );
        }
        System::Mac => {
            roots.extend(
                places
                    .applications
                    .iter()
                    .map(|p| p.join("Unity/Hub/Editor")),
            );
        }
        System::Linux => {
            if let Some(h) = &places.home {
                roots.push(h.join("Unity/Hub/Editor"));
            }
        }
    }
    // Where Unity Hub was told to put them instead.
    let hub_config = match system {
        System::Windows => places.app_data.as_ref().map(|a| a.join("UnityHub")),
        System::Mac => places
            .home
            .as_ref()
            .map(|h| h.join("Library/Application Support/UnityHub")),
        System::Linux => places.home.as_ref().map(|h| h.join(".config/UnityHub")),
    };
    if let Some(chosen) = hub_config
        .and_then(|c| std::fs::read_to_string(c.join("secondaryInstallPath.json")).ok())
        .map(|s| s.trim().trim_matches('"').replace("\\\\", "\\"))
        .filter(|s| !s.is_empty())
    {
        roots.push(PathBuf::from(chosen));
    }
    let inside = match system {
        System::Windows => r"Editor\Data\Tools\UnityYAMLMerge.exe",
        System::Mac => "Unity.app/Contents/Tools/UnityYAMLMerge",
        System::Linux => "Editor/Data/Tools/UnityYAMLMerge",
    };
    let mut out: Vec<UnityEditor> = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for e in entries.flatten() {
            let version = e.file_name().to_string_lossy().to_string();
            let yaml_merge = e.path().join(inside);
            if is_program(&yaml_merge) && !out.iter().any(|o| o.version == version) {
                out.push(UnityEditor {
                    version,
                    yaml_merge,
                });
            }
        }
    }
    out.sort_by(|a, b| unity_order(&b.version, &a.version));
    out
}

/// Orders Unity versions ("2022.3.10f1", "6000.0.23f1") by their numbers.
pub fn unity_order(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |v: &str| -> Vec<u64> {
        v.split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect()
    };
    parts(a).cmp(&parts(b))
}

/// What fills a command's placeholders.
#[derive(Debug, Clone, Default)]
pub struct Vars {
    pub base: String,
    pub local: String,
    pub remote: String,
    pub merged: String,
    pub file: String,
    pub line: usize,
}

impl Vars {
    fn pairs(&self) -> [(&'static str, String); 6] {
        [
            ("BASE", self.base.clone()),
            ("LOCAL", self.local.clone()),
            ("REMOTE", self.remote.clone()),
            ("MERGED", self.merged.clone()),
            ("FILE", self.file.clone()),
            ("LINE", self.line.max(1).to_string()),
        ]
    }

    /// `word` with its placeholders filled in.
    fn fill(&self, word: &str) -> String {
        let mut out = word.to_string();
        for (name, value) in self.pairs() {
            out = out.replace(&format!("${{{name}}}"), &value);
            out = out.replace(&format!("${name}"), &value);
        }
        out
    }
}

/// A program to start: run as it is (no shell), with these variables set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl Run {
    /// How it reads, for showing (and for error messages).
    pub fn describe(&self) -> String {
        std::iter::once(self.program.display().to_string())
            .chain(self.args.iter().cloned())
            .map(|w| {
                if w.contains(' ') {
                    format!("\"{w}\"")
                } else {
                    w
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn command(&self) -> std::process::Command {
        let mut c = std::process::Command::new(&self.program);
        c.args(&self.args);
        for (k, v) in &self.env {
            c.env(k, v);
        }
        c
    }
}

impl Found {
    /// Its command for `role`, filled in.
    pub fn run(&self, role: Role, vars: &Vars) -> Option<Run> {
        let args = self.preset.args(role)?;
        Some(Run {
            program: self.program.clone(),
            args: args.iter().map(|a| vars.fill(a)).collect(),
            env: Vec::new(),
        })
    }
}

/// Splits a command line into words: spaces separate them, except inside
/// double quotes (or single quotes).
pub fn words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut word, mut quote, mut any) = (String::new(), None::<char>, false);
    for ch in line.chars() {
        match (ch, quote) {
            ('"' | '\'', None) => {
                quote = Some(ch);
                any = true;
            }
            (c, Some(q)) if c == q => quote = None,
            (c, None) if c.is_whitespace() => {
                if any {
                    out.push(std::mem::take(&mut word));
                    any = false;
                }
            }
            (c, _) => {
                word.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(word);
    }
    out
}

/// A command line typed in Settings ("custom"), filled in: its first word
/// is the program. `None` when it's empty.
pub fn custom(line: &str, vars: &Vars) -> Option<Run> {
    let mut w = words(line).into_iter().map(|w| vars.fill(&w));
    let program = w.next()?;
    Some(Run {
        program: PathBuf::from(program),
        args: w.collect(),
        env: Vec::new(),
    })
}

/// What's missing from a custom command: the placeholders `role` needs.
pub fn missing_placeholders(line: &str, role: Role) -> Vec<&'static str> {
    let needed: &[&'static str] = match role {
        Role::Merge => &["$MERGED"],
        Role::Diff => &["$LOCAL", "$REMOTE"],
        Role::Editor => &["$FILE"],
    };
    needed
        .iter()
        .copied()
        .filter(|p| !line.contains(p) && !line.contains(&format!("${{{}}}", &p[1..])))
        .collect()
}

/// Git's own choice of tool for merging or diffing (`merge.tool`,
/// `diff.tool`), and how it's set up.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitTool {
    /// Its name in Git's config ("kdiff3", "bc4", "mytool").
    pub name: String,
    /// `mergetool.<name>.cmd`: a shell command of Git's.
    pub cmd: Option<String>,
    /// `mergetool.<name>.path`: where the program is.
    pub path: Option<String>,
}

impl GitTool {
    /// The name to show: the preset's ("Beyond Compare") or Git's.
    pub fn label(&self) -> String {
        PRESETS
            .iter()
            .find(|p| p.is_called(&self.name))
            .map_or_else(|| self.name.clone(), |p| p.name.to_string())
    }
}

/// Git's tool for `role` (merge or diff), from the repository's config (or
/// the global one when `repo` is `None`). A diff with no `diff.tool` uses
/// the merge tool, as Git does.
pub fn git_tool(repo: Option<&str>, role: Role) -> Option<GitTool> {
    let config = match repo.and_then(|r| git2::Repository::discover(r).ok()) {
        Some(r) => r.config().ok()?.snapshot().ok()?,
        None => git2::Config::open_default().ok()?.snapshot().ok()?,
    };
    let get = |k: &str| config.get_string(k).ok().filter(|v| !v.trim().is_empty());
    let (key, section) = match role {
        Role::Merge => ("merge.tool", "mergetool"),
        Role::Diff => ("diff.tool", "difftool"),
        Role::Editor => return None,
    };
    let name = get(key).or_else(|| {
        if role == Role::Diff {
            get("merge.tool")
        } else {
            None
        }
    })?;
    let from = |s: &str| GitTool {
        cmd: get(&format!("{s}.{name}.cmd")),
        path: get(&format!("{s}.{name}.path")),
        name: name.clone(),
    };
    let mut tool = from(section);
    if role == Role::Diff && tool.cmd.is_none() && tool.path.is_none() {
        let merge = from("mergetool");
        tool.path = merge.path;
    }
    Some(tool)
}

/// How to start Git's tool for `role`: its own shell command, or a preset
/// at its configured path or found here. `Err` says why it can't be.
pub fn git_tool_run(
    tool: &GitTool,
    role: Role,
    vars: &Vars,
    places: &Places,
) -> std::result::Result<Run, String> {
    if let Some(cmd) = &tool.cmd {
        let shell = shell(places).ok_or("Git's command needs a shell (sh), which wasn't found")?;
        let mut env: Vec<(String, String)> = vars
            .pairs()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        env.retain(|(k, _)| k != "FILE");
        return Ok(Run {
            program: shell,
            args: vec!["-c".into(), cmd.clone()],
            env,
        });
    }
    let preset = PRESETS
        .iter()
        .find(|p| p.is_called(&tool.name))
        .ok_or_else(|| {
            format!(
                "Git's {} tool \"{}\" isn't one Git Raccoon knows, and has no command set",
                if role == Role::Merge { "merge" } else { "diff" },
                tool.name
            )
        })?;
    let found = match &tool.path {
        Some(p) => Found {
            preset,
            program: PathBuf::from(p),
            version: None,
        },
        None => find(preset, places).ok_or_else(|| format!("{} wasn't found", preset.name))?,
    };
    found
        .run(role, vars)
        .ok_or_else(|| format!("{} can't do that", preset.name))
}

/// A POSIX shell: `sh` on the PATH, or Git for Windows' own.
fn shell(places: &Places) -> Option<PathBuf> {
    if let Some(sh) = places.on_path("sh") {
        return Some(sh);
    }
    let git = places.on_path("git")?;
    // …\Git\cmd\git.exe → …\Git\bin\sh.exe
    let root = git.parent()?.parent()?;
    [root.join(r"bin\sh.exe"), root.join(r"usr\bin\sh.exe")]
        .into_iter()
        .find(|p| p.is_file())
}

/// Every choice found, by id: for showing a settings list once.
pub fn found_by_id(places: &Places) -> BTreeMap<&'static str, Found> {
    PRESETS
        .iter()
        .filter_map(|p| find(p, places).map(|f| (p.id, f)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    fn vars() -> Vars {
        Vars {
            base: "/t/a.BASE.txt".into(),
            local: "/t/a.LOCAL.txt".into(),
            remote: "/t/a.REMOTE.txt".into(),
            merged: "/r/my file.txt".into(),
            file: "/r/my file.txt".into(),
            line: 12,
        }
    }

    /// A folder to pretend programs are installed in.
    fn fake_root(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("git-raccoon-tools-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn install(at: &Path) {
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, "").unwrap();
    }

    #[test]
    fn every_preset_has_the_placeholders_its_roles_need() {
        for p in PRESETS {
            assert!(p.does(Role::Merge) || p.does(Role::Diff) || p.does(Role::Editor));
            if let Some(args) = p.merge {
                let line = args.join(" ");
                for needed in ["$BASE", "$LOCAL", "$REMOTE", "$MERGED"] {
                    assert!(line.contains(needed), "{} merge lacks {needed}", p.id);
                }
            }
            if let Some(args) = p.diff {
                let line = args.join(" ");
                assert!(
                    line.contains("$LOCAL") && line.contains("$REMOTE"),
                    "{}",
                    p.id
                );
            }
            if let Some(args) = p.edit {
                assert!(args.join(" ").contains("$FILE"), "{}", p.id);
            }
        }
    }

    #[test]
    fn fills_in_commands_word_by_word() {
        let kdiff3 = Found {
            preset: preset_by_id("kdiff3").unwrap(),
            program: "/usr/bin/kdiff3".into(),
            version: None,
        };
        let run = kdiff3.run(Role::Merge, &vars()).unwrap();
        // A path with a space stays one argument.
        assert!(run.args.contains(&"/r/my file.txt".to_string()));
        assert_eq!(run.args.last().unwrap(), "/t/a.REMOTE.txt");
        assert!(kdiff3.run(Role::Editor, &vars()).is_none());

        let code = Found {
            preset: preset_by_id("code").unwrap(),
            program: "code".into(),
            version: None,
        };
        let edit = code.run(Role::Editor, &vars()).unwrap();
        assert_eq!(edit.args, ["--goto", "/r/my file.txt:12"]);
        let bc = Found {
            preset: preset_by_id("bcompare").unwrap(),
            program: "bcomp".into(),
            version: None,
        };
        let run = bc.run(Role::Merge, &vars()).unwrap();
        assert_eq!(run.args.last().unwrap(), "-mergeoutput=/r/my file.txt");
        assert_eq!(
            run.describe(),
            "bcomp /t/a.LOCAL.txt /t/a.REMOTE.txt /t/a.BASE.txt \"-mergeoutput=/r/my file.txt\""
        );
    }

    #[test]
    fn reads_custom_commands() {
        let run = custom(
            r#""C:\Tools\My Merge\merge.exe" -o $MERGED ${BASE} '$LOCAL' $REMOTE"#,
            &vars(),
        )
        .unwrap();
        assert_eq!(run.program, PathBuf::from(r"C:\Tools\My Merge\merge.exe"));
        assert_eq!(
            run.args,
            [
                "-o",
                "/r/my file.txt",
                "/t/a.BASE.txt",
                "/t/a.LOCAL.txt",
                "/t/a.REMOTE.txt"
            ]
        );
        assert!(custom("   ", &vars()).is_none());
        assert_eq!(
            missing_placeholders("tool $BASE $LOCAL", Role::Merge),
            ["$MERGED"]
        );
        assert!(missing_placeholders("ed +$LINE ${FILE}", Role::Editor).is_empty());
        assert_eq!(missing_placeholders("diff $LOCAL", Role::Diff), ["$REMOTE"]);
    }

    #[test]
    fn finds_tools_where_each_system_puts_them() {
        let root = fake_root("find");
        // Linux: on the PATH, and JetBrains Toolbox's scripts.
        let bin = root.join("bin");
        install(&bin.join("meld"));
        install(&root.join("home/.local/share/JetBrains/Toolbox/scripts/rider"));
        let linux = Places {
            system: Some(System::Linux),
            path: vec![bin.clone()],
            home: Some(root.join("home")),
            ..Default::default()
        };
        let merge: Vec<&str> = find_all(Role::Merge, &linux)
            .iter()
            .map(|f| f.preset.id)
            .collect();
        assert_eq!(merge, ["meld", "rider"]);
        assert!(
            find_all(Role::Editor, &linux)
                .iter()
                .any(|f| f.preset.id == "rider")
        );
        assert_eq!(
            find(preset_by_id("meld").unwrap(), &linux).unwrap().program,
            bin.join("meld")
        );

        // Windows: Program Files and Local AppData; ".exe" on the PATH.
        let pf = root.join("Program Files");
        install(&pf.join(r"Beyond Compare 4\BComp.exe"));
        // (Joined the way the lookup joins them, whatever the separator here.)
        install(
            &root
                .join("Local")
                .join(r"Programs\Microsoft VS Code\bin\code.cmd"),
        );
        install(&bin.join("p4merge.exe"));
        let windows = Places {
            system: Some(System::Windows),
            path: vec![bin.clone()],
            program_files: vec![pf.clone()],
            local_app_data: Some(root.join("Local")),
            ..Default::default()
        };
        let ids: Vec<&str> = find_all(Role::Diff, &windows)
            .iter()
            .map(|f| f.preset.id)
            .collect();
        assert_eq!(ids, ["bcompare", "p4merge", "code"]);

        // macOS: in /Applications.
        let apps = root.join("Applications");
        install(&apps.join("Sublime Text.app/Contents/SharedSupport/bin/subl"));
        let mac = Places {
            system: Some(System::Mac),
            applications: vec![apps],
            ..Default::default()
        };
        let ids: Vec<&str> = find_all(Role::Editor, &mac)
            .iter()
            .map(|f| f.preset.id)
            .collect();
        assert_eq!(ids, ["sublime"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn finds_unity_editors_newest_first() {
        let root = fake_root("unity");
        let hub = root.join("home/Unity/Hub/Editor");
        for v in ["2021.3.5f1", "6000.0.23f1", "2022.3.10f1"] {
            install(&hub.join(v).join("Editor/Data/Tools/UnityYAMLMerge"));
        }
        // One without the tool doesn't count.
        std::fs::create_dir_all(hub.join("2023.1.0f1")).unwrap();
        // Unity Hub told to put them elsewhere.
        let elsewhere = root.join("Big Disk/Unity");
        install(&elsewhere.join("2022.3.40f1/Editor/Data/Tools/UnityYAMLMerge"));
        let config = root.join("home/.config/UnityHub");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(
            config.join("secondaryInstallPath.json"),
            format!("\"{}\"", elsewhere.display()),
        )
        .unwrap();
        let places = Places {
            system: Some(System::Linux),
            home: Some(root.join("home")),
            ..Default::default()
        };
        let versions: Vec<String> = unity_editors(&places)
            .into_iter()
            .map(|e| e.version)
            .collect();
        assert_eq!(
            versions,
            ["6000.0.23f1", "2022.3.40f1", "2022.3.10f1", "2021.3.5f1"]
        );
        let found = find(preset_by_id(UNITY).unwrap(), &places).unwrap();
        assert_eq!(found.version.as_deref(), Some("6000.0.23f1"));
        let run = found.run(Role::Merge, &vars()).unwrap();
        assert_eq!(run.args[..2], ["merge", "-p"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_gits_own_tool_settings() {
        let t = TempRepo::new();
        let mut config = t.repo.config().unwrap();
        assert!(git_tool(Some(t.path()), Role::Merge).is_none_or(|g| !g.name.is_empty()));
        config.set_str("merge.tool", "bc4").unwrap();
        config
            .set_str("mergetool.bc4.path", "/opt/bc/bcomp")
            .unwrap();
        let merge = git_tool(Some(t.path()), Role::Merge).unwrap();
        assert_eq!(merge.name, "bc4");
        assert_eq!(merge.label(), "Beyond Compare");
        let run = git_tool_run(&merge, Role::Merge, &vars(), &Places::default()).unwrap();
        assert_eq!(run.program, PathBuf::from("/opt/bc/bcomp"));
        // No diff.tool: the merge tool, at its path.
        let diff = git_tool(Some(t.path()), Role::Diff).unwrap();
        assert_eq!(diff.name, "bc4");
        assert_eq!(diff.path.as_deref(), Some("/opt/bc/bcomp"));

        // A tool of one's own: Git's shell command, with its variables.
        config.set_str("merge.tool", "mine").unwrap();
        config
            .set_str("mergetool.mine.cmd", "mymerge \"$BASE\" \"$MERGED\"")
            .unwrap();
        let mine = git_tool(Some(t.path()), Role::Merge).unwrap();
        assert_eq!(mine.label(), "mine");
        if cfg!(unix) {
            let run = git_tool_run(&mine, Role::Merge, &vars(), &Places::here()).unwrap();
            assert_eq!(run.args[0], "-c");
            assert!(
                run.env
                    .contains(&("MERGED".into(), "/r/my file.txt".into()))
            );
        }
        // One Git Raccoon doesn't know, with no command: says so.
        config.remove("mergetool.mine.cmd").unwrap();
        let err = git_tool_run(
            &git_tool(Some(t.path()), Role::Merge).unwrap(),
            Role::Merge,
            &vars(),
            &Places::default(),
        )
        .unwrap_err();
        assert!(err.contains("\"mine\""), "{err}");
    }

    #[test]
    fn a_shell_command_runs_with_gits_variables() {
        if !cfg!(unix) {
            return;
        }
        let root = fake_root("shell");
        let out = root.join("out.txt");
        let tool = GitTool {
            name: "echoer".into(),
            cmd: Some(format!(
                "printf '%s|%s' \"$LOCAL\" \"$MERGED\" > '{}'",
                out.display()
            )),
            path: None,
        };
        let run = git_tool_run(&tool, Role::Merge, &vars(), &Places::here()).unwrap();
        assert!(run.command().status().unwrap().success());
        assert_eq!(
            std::fs::read_to_string(&out).unwrap(),
            "/t/a.LOCAL.txt|/r/my file.txt"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn orders_unity_versions_by_number() {
        use std::cmp::Ordering::*;
        assert_eq!(unity_order("2022.3.10f1", "2022.3.9f1"), Greater);
        assert_eq!(unity_order("6000.0.1f1", "2023.2.20f1"), Greater);
        assert_eq!(unity_order("2021.3.5f1", "2021.3.5f1"), Equal);
    }
}
