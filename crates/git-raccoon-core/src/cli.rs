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
    git_asking(path, true)
}

/// A `git` command that never asks anyone anything: no password or
/// passphrase dialogs, no credential manager windows. For work the user
/// didn't start (fetching in the background): what needs asking fails.
fn git_quiet(path: &str) -> Command {
    let mut cmd = git_asking(path, false);
    cmd.env("GCM_INTERACTIVE", "never")
        .env("SSH_ASKPASS_REQUIRE", "never");
    cmd
}

/// Whether the app's Git remembers how conflicts were resolved (rerere),
/// whatever the user's own config says.
static REMEMBER: AtomicBool = AtomicBool::new(false);

/// The `ssh` the app's own Git commands use, when it isn't Git's usual one
/// (on Windows: Windows' OpenSSH, whose agent has the keys).
static SSH_COMMAND: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Sets (or clears) the `ssh` for the app's own Git commands; the user's
/// own `GIT_SSH_COMMAND` still wins.
pub fn set_ssh_command(ssh: Option<String>) {
    if let Ok(mut s) = SSH_COMMAND.lock() {
        *s = ssh;
    }
}

/// The `ssh` set for the app's Git, if any.
pub fn ssh_command() -> Option<String> {
    SSH_COMMAND.lock().ok().and_then(|s| s.clone())
}

/// The name and email the app's Git commits as (the profile in use), when
/// set; otherwise Git's own config decides.
static IDENTITY: std::sync::Mutex<Option<(String, String)>> = std::sync::Mutex::new(None);

/// Sets (or clears) who the app's Git commits as. Only for its own
/// commands: the user's Git config is left as it is.
pub fn set_identity(who: Option<(String, String)>) {
    if let Ok(mut i) = IDENTITY.lock() {
        *i = who;
    }
}

fn identity() -> Option<(String, String)> {
    IDENTITY.lock().ok().and_then(|i| i.clone())
}

/// Variables for a terminal of the app's, so the Git run in it is like the
/// app's own: commits by the profile in use, sign-in questions answered by
/// the app (with its accounts) unless the person has their own askpass, and
/// the SSH program chosen for the app.
pub fn terminal_env() -> Vec<(String, String)> {
    let mut env = Vec::new();
    if let Some((name, email)) = identity() {
        for who in ["AUTHOR", "COMMITTER"] {
            env.push((format!("GIT_{who}_NAME"), name.clone()));
            env.push((format!("GIT_{who}_EMAIL"), email.clone()));
        }
    }
    if let Some(ask) = crate::askpass::current() {
        env.push((crate::askpass::ENV.to_string(), ask.value.clone()));
        if unset("GIT_ASKPASS") {
            env.push((
                "GIT_ASKPASS".into(),
                ask.program.to_string_lossy().to_string(),
            ));
        }
    }
    if let Some(ssh) = ssh_command().filter(|_| unset("GIT_SSH_COMMAND") && unset("GIT_SSH")) {
        env.push((
            "GIT_SSH_COMMAND".into(),
            format!("\"{}\"", ssh.replace('\\', "/")),
        ));
    }
    env
}

/// Git's own `user.name` and `user.email` (global), where set.
pub fn global_identity() -> (String, String) {
    let Ok(config) = git2::Config::open_default() else {
        return Default::default();
    };
    let get = |key: &str| config.get_string(key).unwrap_or_default();
    (get("user.name"), get("user.email"))
}

/// Turns remembering conflict resolutions on or off for the app's Git.
pub fn set_remember(on: bool) {
    REMEMBER.store(on, Ordering::Relaxed);
}

pub(crate) fn remembering() -> bool {
    REMEMBER.load(Ordering::Relaxed)
}

fn git_asking(path: &str, ask: bool) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(path);
    if remembering() {
        cmd.args(["-c", "rerere.enabled=true"]);
    }
    // Commits (and tags, merges, stashes) are by the profile in use; a
    // rebase or cherry-pick keeps each commit's author.
    if let Some((name, email)) = identity() {
        cmd.arg("-c")
            .arg(format!("user.name={name}"))
            .arg("-c")
            .arg(format!("user.email={email}"));
    }
    cmd.env("GIT_EDITOR", "true")
        .env("GIT_TERMINAL_PROMPT", "0")
        // Buffer output even into a pipe: by default Git flushes after every
        // record there, which makes a million-commit walk twice as slow.
        .env("GIT_FLUSH", "0");
    // Windows' own ssh (say), quoted for Git's shell, when set for the app.
    let chosen = ssh_command()
        .filter(|_| unset("GIT_SSH_COMMAND") && unset("GIT_SSH"))
        .map(|p| format!("\"{}\"", p.replace('\\', "/")));
    if let Some(ssh) = &chosen {
        cmd.env("GIT_SSH_COMMAND", ssh);
    }
    if let Some(ask) = crate::askpass::current().filter(|_| ask) {
        // Questions (passwords, passphrases, new host keys) go to the app.
        // A user's own askpass programs win.
        cmd.env(crate::askpass::ENV, &ask.value);
        if unset("GIT_ASKPASS") {
            cmd.env("GIT_ASKPASS", &ask.program);
        }
        if unset("SSH_ASKPASS") {
            cmd.env("SSH_ASKPASS", &ask.program)
                .env("SSH_ASKPASS_REQUIRE", "force");
        }
    } else if std::env::var_os("GIT_SSH_COMMAND").is_none() && std::env::var_os("GIT_SSH").is_none()
    {
        // With nobody to ask, SSH would wait for a passphrase or host key on
        // a terminal the app doesn't have (or, started from one, block on it
        // unseen). Keys in an agent and known hosts still work.
        // The user's own core.sshCommand stays, with the option added.
        let theirs = users_ssh_command(path);
        let ssh = chosen.as_deref().or(theirs.as_deref()).unwrap_or("ssh");
        cmd.env("GIT_SSH_COMMAND", format!("{ssh} -o BatchMode=yes"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Lets `ssh-add` ask for a key's passphrase in the app (it has no
/// `BatchMode`: with nobody to ask, it fails rather than wait).
pub(crate) fn asking_add(cmd: &mut Command) {
    if let Some(ask) = crate::askpass::current() {
        cmd.env(crate::askpass::ENV, &ask.value);
        if unset("SSH_ASKPASS") {
            cmd.env("SSH_ASKPASS", &ask.program)
                .env("SSH_ASKPASS_REQUIRE", "force");
        }
    } else {
        cmd.env("SSH_ASKPASS_REQUIRE", "never");
    }
}

/// Lets a plain `ssh` (not through Git) ask its questions (passphrases, new
/// hosts) in the app, as Git's do; with nobody to ask, it never waits on a
/// terminal (`BatchMode`).
pub(crate) fn asking_env(cmd: &mut Command) {
    match crate::askpass::current() {
        Some(ask) => {
            cmd.env(crate::askpass::ENV, &ask.value);
            if unset("SSH_ASKPASS") {
                cmd.env("SSH_ASKPASS", &ask.program)
                    .env("SSH_ASKPASS_REQUIRE", "force");
            }
        }
        None => {
            cmd.args(["-o", "BatchMode=yes"]);
        }
    }
}

/// The repository's (or user's) `core.sshCommand`: setting
/// `GIT_SSH_COMMAND` would otherwise hide it. Read fresh each time (it can
/// change while the app runs), in-process.
fn users_ssh_command(path: &str) -> Option<String> {
    let config = match git2::Repository::discover(path) {
        Ok(repo) => repo.config().ok()?,
        Err(_) => git2::Config::open_default().ok()?,
    };
    config
        .get_string("core.sshCommand")
        .ok()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
}

/// Whether an environment variable is unset or empty (Git ignores empty ones).
fn unset(name: &str) -> bool {
    std::env::var_os(name).is_none_or(|v| v.is_empty())
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
    run_env(path, args, stdin, &[])
}

/// [`run`], never asking for credentials (see [`git_quiet`]).
pub fn run_quiet(path: &str, args: &[&str]) -> Result<String> {
    let output = spawn(
        git_quiet(path)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )?
    .wait_with_output()
    .map_err(|e| Error::Command(format!("git did not finish: {e}")))?;
    let text = combine(
        &String::from_utf8_lossy(&output.stderr),
        &String::from_utf8_lossy(&output.stdout),
    );
    finish(args, output.status.success(), output.status, text)
}

/// [`run`], with extra environment variables for git.
pub fn run_env(
    path: &str,
    args: &[&str],
    stdin: Option<&str>,
    env: &[(&str, &str)],
) -> Result<String> {
    let mut child = spawn(
        git(path)
            .envs(env.iter().copied())
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
        // Git may finish before reading it all (a hook refusing a commit,
        // say): then what it printed is the news, not the closed pipe.
        if let Err(e) = pipe.write_all(text.as_bytes())
            && e.kind() != std::io::ErrorKind::BrokenPipe
        {
            return Err(Error::Command(format!("could not send input to git: {e}")));
        }
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

/// Runs `git <args>` in `path`, feeding `stdin`, and passes each line of
/// its output to `each_line` as it arrives (for output too big to hold as
/// one string). A failure becomes [`Error::Command`] with git's complaint.
pub fn run_lines(
    path: &str,
    args: &[&str],
    stdin: &str,
    each_line: &mut dyn FnMut(&str) -> Result<()>,
) -> Result<()> {
    use std::io::BufRead as _;
    let mut child = spawn(
        git(path)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )?;
    // Feed stdin from another thread, so a full stdout pipe can't block it.
    let input = stdin.to_string();
    let mut pipe = child.stdin.take();
    let feeder = std::thread::spawn(move || {
        if let Some(p) = &mut pipe {
            let _ = p.write_all(input.as_bytes());
        }
    });
    let errors = child.stderr.take().map(pump);
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Command("could not read git's output".into()))?;
    let mut reader = std::io::BufReader::with_capacity(1 << 20, stdout);
    let mut line = String::new();
    let mut failed = None;
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if let Err(e) = each_line(line.trim_end()) {
                    failed = Some(e);
                    let _ = child.kill();
                    break;
                }
            }
            Err(e) => {
                failed = Some(Error::Command(format!("could not read git's output: {e}")));
                break;
            }
        }
    }
    let _ = feeder.join();
    let status = child
        .wait()
        .map_err(|e| Error::Command(format!("git did not finish: {e}")))?;
    if let Some(e) = failed {
        return Err(e);
    }
    if status.success() {
        return Ok(());
    }
    let mut text = Vec::new();
    if let Some(rx) = errors {
        while let Ok(chunk) = rx.recv_timeout(Duration::from_millis(200)) {
            text.extend(chunk);
        }
    }
    let text = String::from_utf8_lossy(&text).trim().to_string();
    finish(args, false, status, text).map(drop)
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
    fn gives_the_apps_git_the_chosen_ssh() {
        if !unset("GIT_SSH_COMMAND") || !unset("GIT_SSH") {
            return;
        }
        let env_of = |cmd: &Command| {
            cmd.get_envs()
                .find(|(k, _)| *k == "GIT_SSH_COMMAND")
                .and_then(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()))
        };
        set_ssh_command(Some(r"C:\Windows\System32\OpenSSH\ssh.exe".into()));
        let with = env_of(&git_asking(".", false));
        set_ssh_command(None);
        let without = env_of(&git_asking(".", false));
        // Quoted, with forward slashes, for Git's shell.
        assert_eq!(
            with.as_deref(),
            Some(r#""C:/Windows/System32/OpenSSH/ssh.exe" -o BatchMode=yes"#)
        );
        assert_eq!(without.as_deref(), Some("ssh -o BatchMode=yes"));
    }

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
