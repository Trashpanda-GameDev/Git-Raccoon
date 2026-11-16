//! Opening a terminal in a repository's folder: the system's own, or the
//! command set in Settings.

use std::path::Path;
use std::process::{Command, Stdio};

/// Which system's ways to try.
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

/// Splits a command line into words: spaces separate them, except inside
/// double quotes.
fn words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut word, mut quoted, mut any) = (String::new(), false, false);
    for ch in line.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut word));
                    any = false;
                }
            }
            c => {
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

/// The commands to try, in order, to open a terminal in `dir`: the one set
/// in Settings (`{path}` stands for the folder) if there is one, otherwise
/// the system's usual ones. Each is started in `dir` too.
pub fn candidates(
    dir: &str,
    custom: &str,
    system: System,
    env_terminal: Option<&str>,
) -> Vec<Vec<String>> {
    let custom = custom.trim();
    if !custom.is_empty() {
        let line = words(custom)
            .into_iter()
            .map(|w| w.replace("{path}", dir))
            .collect();
        return vec![line];
    }
    let v = |parts: &[&str]| parts.iter().map(ToString::to_string).collect::<Vec<_>>();
    match system {
        System::Windows => vec![
            v(&["wt.exe", "-d", dir]),
            v(&[r"C:\Program Files\Git\git-bash.exe", &format!("--cd={dir}")]),
            // `start` opens a new window; its first quoted word is the title.
            v(&["cmd.exe", "/c", "start", "Terminal", "/D", dir, "cmd.exe"]),
        ],
        System::Mac => vec![v(&["open", "-a", "Terminal", dir])],
        System::Linux => {
            let mut out = Vec::new();
            if let Some(t) = env_terminal.filter(|t| !t.trim().is_empty()) {
                out.push(words(t));
            }
            out.extend([
                v(&["x-terminal-emulator"]),
                v(&["gnome-terminal", &format!("--working-directory={dir}")]),
                v(&["konsole", "--workdir", dir]),
                v(&["xfce4-terminal", &format!("--working-directory={dir}")]),
                v(&["kitty", "--directory", dir]),
                v(&["alacritty", "--working-directory", dir]),
                v(&["wezterm", "start", "--cwd", dir]),
                v(&["foot", "--working-directory", dir]),
                v(&["xterm"]),
            ]);
            out
        }
    }
}

/// Opens a terminal in `dir`. Returns what was started, or why nothing was.
pub fn open(dir: &str, custom: &str) -> Result<String, String> {
    if !Path::new(dir).is_dir() {
        return Err(format!("{dir} isn't a folder"));
    }
    let env_terminal = std::env::var("TERMINAL").ok();
    let tried = candidates(dir, custom, System::this(), env_terminal.as_deref());
    let mut last = String::from("no terminal was found");
    for line in &tried {
        let Some((program, args)) = line.split_first() else {
            continue;
        };
        let started = Command::new(program)
            .args(args)
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match started {
            // Not waited for: it runs on its own.
            Ok(_) => return Ok(program.clone()),
            Err(e) => last = format!("{program}: {e}"),
        }
    }
    if custom.trim().is_empty() {
        Err(format!("{last}. Set the terminal to use in Settings"))
    } else {
        Err(format!("the terminal in Settings didn't start ({last})"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_command_lines() {
        assert_eq!(
            words(r#"wezterm start --cwd "{path}""#),
            ["wezterm", "start", "--cwd", "{path}"]
        );
        assert_eq!(
            words(r#"  "C:\Program Files\x.exe"  -a  "#),
            [r"C:\Program Files\x.exe", "-a"]
        );
        assert_eq!(words(r#"a "" b"#), ["a", "", "b"]);
        assert!(words("   ").is_empty());
    }

    #[test]
    fn picks_the_terminal_to_try() {
        let custom = candidates(
            "/r/my repo",
            r#"alacritty --working-directory "{path}""#,
            System::Linux,
            None,
        );
        assert_eq!(custom, [["alacritty", "--working-directory", "/r/my repo"]]);
        let linux = candidates("/r", "", System::Linux, Some("kitty --single-instance"));
        assert_eq!(linux[0], ["kitty", "--single-instance"], "$TERMINAL first");
        assert!(linux.contains(&vec![
            "gnome-terminal".to_string(),
            "--working-directory=/r".to_string()
        ]));
        assert_eq!(
            candidates("/r", "", System::Mac, None),
            [["open", "-a", "Terminal", "/r"]]
        );
        let windows = candidates(r"C:\r", "", System::Windows, None);
        assert_eq!(windows[0], ["wt.exe", "-d", r"C:\r"]);
        assert_eq!(windows.len(), 3);
    }

    #[test]
    fn opens_one_in_the_folder() {
        let dir = std::env::temp_dir().join(format!("git-raccoon-terminal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.to_str().unwrap();
        // A "terminal" that leaves a mark where it starts.
        let custom = if cfg!(windows) {
            "cmd.exe /c type nul > opened"
        } else {
            "touch opened"
        };
        assert!(open(path, custom).is_ok());
        let mark = dir.join("opened");
        for _ in 0..100 {
            if mark.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(mark.exists(), "started in the folder");
        assert!(
            open(path, "no-such-terminal-anywhere")
                .unwrap_err()
                .contains("didn't start")
        );
        assert!(open("/no/such/folder", "").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
