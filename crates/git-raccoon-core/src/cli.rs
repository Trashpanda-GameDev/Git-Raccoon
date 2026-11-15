//! Running the user's own `git`, for operations where its behaviour
//! (hooks, signing, config, credentials, conflict handling) should apply
//! exactly.

use std::io::Write as _;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::{Error, Result};

/// A `git` command for the repository at `path`, without a console window
/// on Windows and never waiting on an editor or a terminal prompt.
fn git(path: &str) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(path)
        .env("GIT_EDITOR", "true")
        .env("GIT_TERMINAL_PROMPT", "0");
    // SSH would ask for passphrases and new host keys on a terminal the app
    // doesn't have (or, started from one, block on it unseen). Keys in an
    // agent and known hosts still work. A user's own SSH command wins.
    if std::env::var_os("GIT_SSH_COMMAND").is_none() && std::env::var_os("GIT_SSH").is_none() {
        cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn spawn(cmd: &mut Command) -> Result<Child> {
    cmd.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            Error::Command("Git isn't installed, or isn't on PATH.".into())
        }
        _ => Error::Command(format!("could not run git: {e}")),
    })
}

/// stderr then stdout, trimmed, as one text.
fn combine(stderr: &str, stdout: &str) -> String {
    [stderr, stdout]
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn finish(
    args: &[&str],
    ok: bool,
    status: std::process::ExitStatus,
    text: String,
) -> Result<String> {
    if ok {
        Ok(text)
    } else if text.is_empty() {
        Err(Error::Command(format!(
            "git {} failed ({status})",
            args.first().unwrap_or(&"")
        )))
    } else {
        Err(Error::Command(text))
    }
}

/// Runs `git <args>` in `path`, feeding `stdin` if given. Returns what it
/// printed; a failure becomes [`Error::Command`] with its output.
pub fn run(path: &str, args: &[&str], stdin: Option<&str>) -> Result<String> {
    let mut child = spawn(
        git(path)
            .args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(text.as_bytes())
            .map_err(|e| Error::Command(format!("could not send input to git: {e}")))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| Error::Command(format!("git did not finish: {e}")))?;
    let text = combine(
        &String::from_utf8_lossy(&output.stderr),
        &String::from_utf8_lossy(&output.stdout),
    );
    finish(args, output.status.success(), output.status, text)
}

/// Reads a pipe on its own thread, sending what arrives.
fn pump(mut pipe: impl std::io::Read + Send + 'static) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match pipe.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    rx
}

/// Runs `git <args>` (a fetch, pull or push with `--progress`), passing
/// each progress line to `on_progress` as it arrives. Setting `cancel`
/// stops git and returns [`Error::Cancelled`].
pub fn run_with_progress(
    path: &str,
    args: &[&str],
    on_progress: &mut dyn FnMut(&str),
    cancel: &AtomicBool,
) -> Result<String> {
    let mut child = spawn(
        git(path)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )?;
    let (Some(err_pipe), Some(out_pipe)) = (child.stderr.take(), child.stdout.take()) else {
        return Err(Error::Command("could not read git's output".into()));
    };
    let (stderr, stdout) = (pump(err_pipe), pump(out_pipe));

    // Progress lines end in \r (redrawn in place); finished lines in \n.
    let mut log = String::new();
    let mut partial: Vec<u8> = Vec::new();
    let mut take = |chunk: Vec<u8>, log: &mut String| {
        partial.extend(chunk);
        while let Some(i) = partial.iter().position(|&b| b == b'\r' || b == b'\n') {
            let line = String::from_utf8_lossy(&partial[..i])
                .trim_end()
                .to_string();
            let complete = partial[i] == b'\n';
            partial.drain(..=i);
            if line.trim().is_empty() {
                continue;
            }
            on_progress(&line);
            if complete {
                log.push_str(&line);
                log.push('\n');
            }
        }
    };
    let mut exited_at: Option<Instant> = None;
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Cancelled);
        }
        match stderr.recv_timeout(Duration::from_millis(50)) {
            Ok(chunk) => take(chunk, &mut log),
            Err(RecvTimeoutError::Disconnected) => {
                break child
                    .wait()
                    .map_err(|e| Error::Command(format!("git did not finish: {e}")))?;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        // Something git started (an SSH connection kept open, say) may hold
        // stderr after git itself is done: don't wait on it for long.
        if let Ok(Some(status)) = child.try_wait() {
            let at = *exited_at.get_or_insert_with(Instant::now);
            if at.elapsed() > Duration::from_millis(300) {
                break status;
            }
        }
    };
    while let Ok(chunk) = stderr.try_recv() {
        take(chunk, &mut log);
    }
    let mut out = Vec::new();
    while let Ok(chunk) = stdout.recv_timeout(Duration::from_millis(100)) {
        out.extend(chunk);
    }
    let text = combine(&log, &String::from_utf8_lossy(&out));
    finish(args, status.success(), status, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_progress_and_cancels() {
        let dir = std::env::temp_dir();
        let path = dir.to_str().unwrap();
        let mut lines = Vec::new();
        let out = run_with_progress(
            path,
            &["version"],
            &mut |l| lines.push(l.to_string()),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(out.starts_with("git version"), "{out}");

        let cancel = AtomicBool::new(true);
        let err = run_with_progress(path, &["version"], &mut |_| {}, &cancel).unwrap_err();
        assert!(matches!(err, Error::Cancelled));
    }
}
