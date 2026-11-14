//! Running the user's own `git`, for operations where its behaviour
//! (hooks, signing, config, conflict handling) should apply exactly.

use std::io::Write as _;
use std::process::{Command, Stdio};

use crate::{Error, Result};

/// A `git` command for the repository at `path`, without a console window
/// on Windows and never waiting on an editor or a password prompt.
fn git(path: &str) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(path)
        .env("GIT_EDITOR", "true")
        .env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Runs `git <args>` in `path`, feeding `stdin` if given. Returns what it
/// printed; a failure becomes [`Error::Command`] with its output.
pub fn run(path: &str, args: &[&str], stdin: Option<&str>) -> Result<String> {
    let mut child = git(path)
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                Error::Command("Git isn't installed, or isn't on PATH.".into())
            }
            _ => Error::Command(format!("could not run git: {e}")),
        })?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(text.as_bytes())
            .map_err(|e| Error::Command(format!("could not send input to git: {e}")))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| Error::Command(format!("git did not finish: {e}")))?;
    let text = [&output.stderr, &output.stdout]
        .iter()
        .map(|b| String::from_utf8_lossy(b).trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if output.status.success() {
        Ok(text)
    } else if text.is_empty() {
        Err(Error::Command(format!(
            "git {} failed ({})",
            args.first().unwrap_or(&""),
            output.status
        )))
    } else {
        Err(Error::Command(text))
    }
}
