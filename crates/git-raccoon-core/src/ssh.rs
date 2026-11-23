//! What SSH has and would do, read without changing anything: the keys in
//! `~/.ssh`, the agent and its keys, what SSH would use for a host, and a
//! connection test that says who you're signed in as, or exactly why not.
//!
//! The user's own setup wins throughout: the `ssh` Git would run
//! (`GIT_SSH_COMMAND`, `core.sshCommand`, `GIT_SSH`), their `~/.ssh/config`
//! and their agent.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ssh_key::{HashAlg, PrivateKey, PublicKey};

use crate::{Error, Result};

/// The `ssh` (and `ssh-add`) to run: the one Git would use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ssh {
    /// The program, and any arguments it comes with (`ssh -i key`).
    pub command: Vec<String>,
    /// `ssh-add`, beside that `ssh` when it's given by path.
    pub add: String,
    /// Where it came from, to say so: "GIT_SSH_COMMAND", "core.sshCommand",
    /// "GIT_SSH" or "ssh".
    pub from: &'static str,
}

impl Ssh {
    /// The `ssh` Git would run, in the repository at `path` if given (its
    /// `core.sshCommand` counts), else for the user.
    pub fn for_git(path: Option<&str>) -> Ssh {
        if let Some(c) = env("GIT_SSH_COMMAND") {
            return Ssh::from_command(&c, "GIT_SSH_COMMAND");
        }
        let config = match path {
            Some(p) => crate::cli::run_quiet(p, &["config", "core.sshCommand"]).ok(),
            None => crate::cli::run_quiet(".", &["config", "--global", "core.sshCommand"]).ok(),
        };
        if let Some(c) = config
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
        {
            return Ssh::from_command(&c, "core.sshCommand");
        }
        if let Some(p) = env("GIT_SSH") {
            return Ssh::from_program(&p, "GIT_SSH");
        }
        Ssh::from_program("ssh", "ssh")
    }

    /// A given `ssh` program (by name or path).
    pub fn from_program(program: &str, from: &'static str) -> Ssh {
        Ssh {
            add: beside(program, "ssh-add"),
            command: vec![program.to_string()],
            from,
        }
    }

    /// A shell-style command, as `core.sshCommand` holds one.
    pub fn from_command(command: &str, from: &'static str) -> Ssh {
        let words = split_words(command);
        let program = words.first().cloned().unwrap_or_else(|| "ssh".into());
        Ssh {
            add: beside(&program, "ssh-add"),
            command: if words.is_empty() {
                vec![program]
            } else {
                words
            },
            from,
        }
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.command[0]);
        cmd.args(&self.command[1..]);
        quiet_window(&mut cmd);
        cmd
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// `tool` in the same folder as `program`, when that's a path.
fn beside(program: &str, tool: &str) -> String {
    let p = Path::new(program);
    match p.parent().filter(|d| !d.as_os_str().is_empty()) {
        Some(dir) => {
            let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy()));
            dir.join(format!("{tool}{}", ext.unwrap_or_default()))
                .to_string_lossy()
                .into_owned()
        }
        None => tool.to_string(),
    }
}

/// Splits a command the way a shell would, for quotes and backslashes.
fn split_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut any = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '\'' | '"') => {
                quote = Some(c);
                any = true;
            }
            (None, '\\') | (Some('"'), '\\') => {
                if let Some(next) = chars.next() {
                    word.push(next);
                    any = true;
                }
            }
            (None, c) if c.is_whitespace() => {
                if any {
                    words.push(std::mem::take(&mut word));
                    any = false;
                }
            }
            (_, c) => {
                word.push(c);
                any = true;
            }
        }
    }
    if any {
        words.push(word);
    }
    words
}

#[cfg(windows)]
fn quiet_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    cmd.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn quiet_window(_: &mut Command) {}

/// The user's `~/.ssh` folder.
pub fn ssh_dir() -> Option<PathBuf> {
    let home = env("HOME").or_else(|| env("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".ssh"))
}

/// A key pair in `~/.ssh`, or one `~/.ssh/config` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    /// The private key file.
    pub path: PathBuf,
    /// Its public half, when there's a `.pub` beside it.
    pub public: Option<String>,
    /// "ED25519", "RSA", "ECDSA"…
    pub kind: String,
    /// "SHA256:…", as `ssh-keygen -l` and the agent show it.
    pub fingerprint: Option<String>,
    pub comment: String,
    /// Whether a passphrase protects it (`None`: can't tell).
    pub passphrase: Option<bool>,
    /// Named by an `IdentityFile` in `~/.ssh/config`.
    pub in_config: bool,
}

/// The key pairs in `dir` (usually `~/.ssh`), and the ones its `config`
/// names elsewhere, by file name.
pub fn keys(dir: &Path) -> Vec<Key> {
    let named = config_identity_files(dir);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && looks_private(p))
                .collect()
        })
        .unwrap_or_default();
    for p in &named {
        if p.is_file() && !paths.contains(p) && looks_private(p) {
            paths.push(p.clone());
        }
    }
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let mut k = read_key(&p);
            k.in_config = named.contains(&p);
            k
        })
        .collect()
}

/// Whether a file holds a private key (by its first line).
fn looks_private(path: &Path) -> bool {
    if path.extension().is_some_and(|e| e == "pub") {
        return false;
    }
    let Ok(text) = std::fs::read(path) else {
        return false;
    };
    let head = String::from_utf8_lossy(&text[..text.len().min(80)]).into_owned();
    head.starts_with("-----BEGIN") && head.contains("PRIVATE KEY")
}

/// Reads what's knowable about a key pair without its passphrase.
pub fn read_key(path: &Path) -> Key {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let pub_path = PathBuf::from(format!("{}.pub", path.display()));
    let public_text = std::fs::read_to_string(&pub_path).ok();
    let mut key = Key {
        path: path.to_path_buf(),
        public: public_text.as_ref().map(|t| t.trim().to_string()),
        kind: String::new(),
        fingerprint: None,
        comment: String::new(),
        passphrase: None,
        in_config: false,
    };
    // The OpenSSH format says itself whether it's encrypted, and holds the
    // public key and comment in the clear.
    if let Ok(private) = PrivateKey::from_openssh(&text) {
        key.passphrase = Some(private.is_encrypted());
        describe(&mut key, private.public_key());
        if key.comment.is_empty() {
            key.comment = private.comment().to_string();
        }
    } else if text.contains("BEGIN") {
        // The older PEM formats: encrypted ones say so in a header.
        key.passphrase = Some(text.contains("ENCRYPTED"));
        key.kind = if text.contains("RSA") {
            "RSA".into()
        } else if text.contains("EC PRIVATE") {
            "ECDSA".into()
        } else if text.contains("DSA") {
            "DSA".into()
        } else {
            String::new()
        };
    }
    // The .pub file's comment is the one people see and edit.
    if let Some(p) = public_text
        .as_deref()
        .and_then(|t| PublicKey::from_openssh(t.trim()).ok())
    {
        describe(&mut key, &p);
    }
    key
}

fn describe(key: &mut Key, public: &PublicKey) {
    key.kind = kind_name(public.algorithm().as_str());
    key.fingerprint = Some(public.fingerprint(HashAlg::Sha256).to_string());
    if !public.comment().is_empty() {
        key.comment = public.comment().to_string();
    }
}

/// "ssh-ed25519" → "ED25519", as `ssh-keygen -l` names them.
fn kind_name(algorithm: &str) -> String {
    match algorithm {
        "ssh-ed25519" => "ED25519".into(),
        "ssh-rsa" => "RSA".into(),
        "ssh-dss" => "DSA".into(),
        a if a.starts_with("ecdsa-") => "ECDSA".into(),
        a if a.starts_with("sk-ssh-ed25519") => "ED25519-SK".into(),
        a if a.starts_with("sk-ecdsa") => "ECDSA-SK".into(),
        a => a.to_uppercase(),
    }
}

/// The key files `dir/config` names with `IdentityFile` (`~` expanded,
/// relative ones in `dir`).
fn config_identity_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(dir.join("config")) else {
        return Vec::new();
    };
    let home = dir.parent().map(Path::to_path_buf);
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once(|c: char| c.is_whitespace() || c == '=') else {
            continue;
        };
        if !key.eq_ignore_ascii_case("identityfile") {
            continue;
        }
        let value = value
            .trim_start_matches(|c: char| c.is_whitespace() || c == '=')
            .trim();
        let value = value.trim_matches('"');
        let path = match (value.strip_prefix("~/"), &home) {
            (Some(rest), Some(h)) => h.join(rest),
            _ if Path::new(value).is_absolute() => PathBuf::from(value),
            _ => dir.join(value),
        };
        if !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

/// Which agent SSH talks to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentKind {
    /// OpenSSH's `ssh-agent` (or one like it), at a socket.
    SshAgent,
    /// 1Password's SSH agent.
    OnePassword,
    /// GnuPG's agent, speaking SSH.
    GnuPg,
    /// macOS's own (launchd starts it, the keychain keeps passphrases).
    MacOs,
    /// Windows' OpenSSH Authentication Agent service.
    WindowsOpenSsh,
    /// Pageant (PuTTY's agent), which Git for Windows can use.
    Pageant,
    /// None set up.
    None,
}

impl AgentKind {
    pub fn name(&self) -> &'static str {
        match self {
            AgentKind::SshAgent => "ssh-agent",
            AgentKind::OnePassword => "1Password",
            AgentKind::GnuPg => "GnuPG's agent",
            AgentKind::MacOs => "macOS's agent",
            AgentKind::WindowsOpenSsh => "Windows' OpenSSH agent",
            AgentKind::Pageant => "Pageant",
            AgentKind::None => "no agent",
        }
    }
}

/// Which agent a socket (`SSH_AUTH_SOCK`, or an `IdentityAgent`) is.
pub fn agent_kind(socket: Option<&str>) -> AgentKind {
    let Some(s) = socket.filter(|s| !s.trim().is_empty() && *s != "none") else {
        return if cfg!(windows) {
            AgentKind::WindowsOpenSsh
        } else {
            AgentKind::None
        };
    };
    let lower = s.to_lowercase();
    if lower.contains("1password") {
        AgentKind::OnePassword
    } else if lower.contains("gpg") || lower.contains("gnupg") {
        AgentKind::GnuPg
    } else if lower.contains("com.apple.launchd") {
        AgentKind::MacOs
    } else if lower.contains("openssh-ssh-agent") {
        AgentKind::WindowsOpenSsh
    } else if lower.contains("pageant") {
        AgentKind::Pageant
    } else {
        AgentKind::SshAgent
    }
}

/// A key the agent holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentKey {
    pub bits: u32,
    pub fingerprint: String,
    pub comment: String,
    pub kind: String,
}

/// The agent: which, whether it answers, and its keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    pub kind: AgentKind,
    /// Where SSH reaches it (`SSH_AUTH_SOCK`), if set.
    pub socket: Option<String>,
    pub running: bool,
    pub keys: Vec<AgentKey>,
    /// Why it couldn't be asked, if it couldn't.
    pub problem: Option<String>,
}

/// Asks the agent for its keys (`ssh-add -l`).
pub fn agent(ssh: &Ssh) -> Agent {
    agent_at(ssh, env("SSH_AUTH_SOCK"))
}

/// Asks the agent at `socket` (else the usual place) for its keys.
pub fn agent_at(ssh: &Ssh, socket: Option<String>) -> Agent {
    let kind = agent_kind(socket.as_deref());
    let mut cmd = Command::new(&ssh.add);
    if let Some(s) = &socket {
        cmd.env("SSH_AUTH_SOCK", s);
    }
    cmd.args(["-l", "-E", "sha256"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    quiet_window(&mut cmd);
    let (code, text) = match cmd.output() {
        Ok(o) => (
            o.status.code(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            ),
        ),
        Err(e) => {
            return Agent {
                kind,
                socket,
                running: false,
                keys: Vec::new(),
                problem: Some(format!("{} couldn't be run: {e}", ssh.add)),
            };
        }
    };
    let mut agent = parse_agent(code, &text);
    agent.kind = if agent.running || kind != AgentKind::WindowsOpenSsh {
        kind
    } else {
        // Nothing answered where Windows' agent would be.
        AgentKind::WindowsOpenSsh
    };
    agent.socket = socket;
    agent
}

/// Reads `ssh-add -l -E sha256`'s answer: 0 with keys, 1 with none, 2 when
/// there's no agent to ask.
pub fn parse_agent(code: Option<i32>, text: &str) -> Agent {
    let keys: Vec<AgentKey> = text.lines().filter_map(parse_agent_key).collect();
    let running = code != Some(2) && (code == Some(0) || code == Some(1) || !keys.is_empty());
    let problem = (!running)
        .then(|| text.trim().to_string())
        .filter(|t| !t.is_empty());
    Agent {
        kind: AgentKind::None,
        socket: None,
        running,
        keys,
        problem,
    }
}

/// "256 SHA256:abc… rocky@box (ED25519)"
fn parse_agent_key(line: &str) -> Option<AgentKey> {
    let (bits, rest) = line.trim().split_once(' ')?;
    let bits: u32 = bits.parse().ok()?;
    let (fingerprint, rest) = rest.split_once(' ').unwrap_or((rest, ""));
    if !fingerprint.starts_with("SHA256:") {
        return None;
    }
    let (comment, kind) = match rest.rfind(" (") {
        Some(at) if rest.ends_with(')') => (&rest[..at], &rest[at + 2..rest.len() - 1]),
        _ => (rest, ""),
    };
    Some(AgentKey {
        bits,
        fingerprint: fingerprint.to_string(),
        comment: comment.to_string(),
        kind: kind.to_string(),
    })
}

/// What SSH would use to reach a host (`ssh -G`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostConfig {
    pub hostname: String,
    pub user: String,
    pub port: u16,
    /// The key files it would try, in order (`~` left as it is).
    pub identity_files: Vec<String>,
    /// Only those (`IdentitiesOnly`), not every key in the agent.
    pub identities_only: bool,
    /// The agent it would use instead of `SSH_AUTH_SOCK`, if set.
    pub identity_agent: Option<String>,
    pub add_keys_to_agent: String,
    pub strict_host_key_checking: String,
}

/// What SSH would use for `host` (`ssh -G`), with the user's own config.
pub fn host_config(ssh: &Ssh, host: &str) -> Result<HostConfig> {
    let mut cmd = ssh.command();
    cmd.args(["-G", host])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = cmd
        .output()
        .map_err(|e| Error::Unsupported(format!("{} couldn't be run: {e}", ssh.command[0])))?;
    if !out.status.success() {
        return Err(Error::Command(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    Ok(parse_host_config(&String::from_utf8_lossy(&out.stdout)))
}

pub fn parse_host_config(text: &str) -> HostConfig {
    let mut c = HostConfig {
        port: 22,
        ..HostConfig::default()
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once(' ') else {
            continue;
        };
        let value = value.trim().to_string();
        match key {
            "hostname" => c.hostname = value,
            "user" => c.user = value,
            "port" => c.port = value.parse().unwrap_or(22),
            "identityfile" => c.identity_files.push(value),
            "identitiesonly" => c.identities_only = value == "yes",
            "identityagent" if value != "none" => c.identity_agent = Some(value),
            "addkeystoagent" => c.add_keys_to_agent = value,
            "stricthostkeychecking" => c.strict_host_key_checking = value,
            _ => {}
        }
    }
    c
}

/// How a connection test went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    /// Signed in, as this user if the host says.
    SignedIn(Option<String>),
    /// No key was offered: none found, none in the agent.
    NoKey,
    /// Keys were offered (these, by fingerprint or file), and refused.
    KeyRefused(Vec<String>),
    /// The host's key isn't known yet (and nobody could be asked).
    HostUnknown,
    /// The host's key differs from the one known for it: SSH refuses.
    HostKeyChanged,
    /// No answer on that port, or it refused: often a firewall.
    PortBlocked,
    /// The name doesn't resolve.
    NameNotFound,
    /// Something else, with what SSH said.
    Other(String),
}

impl Connection {
    /// A plain sentence on what happened.
    pub fn describe(&self, host: &str) -> String {
        match self {
            Connection::SignedIn(Some(who)) => format!("Signed in to {host} as {who}."),
            Connection::SignedIn(None) => format!("Signed in to {host}."),
            Connection::NoKey => {
                format!("No key was offered to {host}: none was found, and the agent has none.")
            }
            Connection::KeyRefused(_) => format!(
                "{host} refused the key: it isn't on your account there (or it's on another account)."
            ),
            Connection::HostUnknown => format!("{host}'s key isn't trusted yet."),
            Connection::HostKeyChanged => format!(
                "{host}'s key has changed since it was trusted: SSH won't connect, in case someone is in between."
            ),
            Connection::PortBlocked => {
                format!("{host} didn't answer on the SSH port: a firewall may be blocking it.")
            }
            Connection::NameNotFound => {
                format!("{host} wasn't found: check the name, and the network.")
            }
            Connection::Other(text) => format!("Couldn't connect to {host}: {text}"),
        }
    }
}

/// Tests signing in to `host` (as `user`, "git" for forges): `ssh -T`,
/// asking for passphrases and new hosts in the app when it can.
pub fn test_connection(ssh: &Ssh, user: &str, host: &str, port: Option<u16>) -> Result<Connection> {
    let mut cmd = ssh.command();
    cmd.args(["-T", "-v", "-o", "ConnectTimeout=10"]);
    if let Some(p) = port {
        cmd.args(["-p", &p.to_string()]);
    }
    crate::cli::asking_env(&mut cmd);
    cmd.arg(format!("{user}@{host}"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = cmd
        .output()
        .map_err(|e| Error::Unsupported(format!("{} couldn't be run: {e}", ssh.command[0])))?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(classify(out.status.code(), &text))
}

/// Reads what `ssh -T -v` said (and its exit code) into what happened.
pub fn classify(code: Option<i32>, text: &str) -> Connection {
    let lower = text.to_lowercase();
    // Signed in: forges say who, then end the session (often exit 1).
    if let Some(who) = signed_in_as(text) {
        return Connection::SignedIn(Some(who));
    }
    if lower.contains("successfully authenticated")
        || lower.contains("authenticated via ssh key")
        || lower.contains("authenticated to ") && lower.contains("using \"publickey\"")
    {
        return Connection::SignedIn(None);
    }
    if lower.contains("remote host identification has changed") {
        return Connection::HostKeyChanged;
    }
    if lower.contains("host key verification failed")
        || lower.contains("no ") && lower.contains("host key is known")
    {
        return Connection::HostUnknown;
    }
    if lower.contains("could not resolve hostname")
        || lower.contains("name or service not known")
        || lower.contains("nodename nor servname")
        || lower.contains("no such host is known")
    {
        return Connection::NameNotFound;
    }
    if lower.contains("connection timed out")
        || lower.contains("operation timed out")
        || lower.contains("connection refused")
        || lower.contains("network is unreachable")
        || lower.contains("kex_exchange_identification")
    {
        return Connection::PortBlocked;
    }
    if lower.contains("permission denied") {
        let offered: Vec<String> = text
            .lines()
            .filter_map(|l| l.split_once("Offering public key: "))
            .map(|(_, rest)| {
                // "/home/rocky/.ssh/id_ed25519 ED25519 SHA256:… agent"
                rest.split_whitespace()
                    .find(|w| w.starts_with("SHA256:"))
                    .unwrap_or_else(|| rest.split_whitespace().next().unwrap_or(rest))
                    .to_string()
            })
            .collect();
        return if offered.is_empty() {
            Connection::NoKey
        } else {
            Connection::KeyRefused(offered)
        };
    }
    if code == Some(0) {
        return Connection::SignedIn(None);
    }
    // The last thing SSH said that isn't its debugging.
    let said = text
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("debug") && !l.starts_with("OpenSSH_"))
        .unwrap_or("it stopped without saying why")
        .to_string();
    Connection::Other(said)
}

/// Who a forge says you're signed in as.
fn signed_in_as(text: &str) -> Option<String> {
    let word = |s: &str| -> Option<String> {
        let w: String = s
            .chars()
            .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
            .collect();
        let w = w.trim_end_matches('.').to_string();
        (!w.is_empty()).then_some(w)
    };
    for line in text.lines() {
        let line = line.trim();
        // GitHub: "Hi rocky! You've successfully authenticated…"
        // Gitea/Forgejo: "Hi there, rocky! You've successfully authenticated…"
        if let Some(rest) = line
            .strip_prefix("Hi there, ")
            .or_else(|| line.strip_prefix("Hi "))
            && rest.contains("successfully authenticated")
        {
            return word(rest);
        }
        // GitLab: "Welcome to GitLab, @rocky!"
        if let Some(at) = line.find("Welcome to GitLab, @") {
            return word(&line[at + "Welcome to GitLab, @".len()..]);
        }
        // Bitbucket: "logged in as rocky."
        if let Some(at) = line.find("logged in as ") {
            return word(&line[at + "logged in as ".len()..]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_commands_like_a_shell() {
        assert_eq!(
            split_words(r#"ssh -i "/c/My Keys/id" -o 'A B'  x\ y"#),
            ["ssh", "-i", "/c/My Keys/id", "-o", "A B", "x y"]
        );
        assert_eq!(
            Ssh::from_command(
                r#""C:/Program Files/OpenSSH/ssh.exe" -v"#,
                "core.sshCommand"
            )
            .add,
            Path::new("C:/Program Files/OpenSSH")
                .join("ssh-add.exe")
                .to_string_lossy()
        );
        assert_eq!(Ssh::from_program("ssh", "ssh").add, "ssh-add");
    }

    #[test]
    fn reads_what_the_agent_holds() {
        let a = parse_agent(
            Some(0),
            "256 SHA256:/NSQAuG/3kXawpE0NdetJ/SpxFBkjZForcVjurzNQC8 rocky@box (ED25519)\n3072 SHA256:abc work laptop (RSA)\n",
        );
        assert!(a.running);
        assert_eq!(a.keys.len(), 2);
        assert_eq!(a.keys[1].comment, "work laptop");
        assert_eq!(a.keys[1].kind, "RSA");
        assert_eq!(a.keys[0].bits, 256);
        let empty = parse_agent(Some(1), "The agent has no identities.\n");
        assert!(empty.running && empty.keys.is_empty());
        let none = parse_agent(
            Some(2),
            "Could not open a connection to your authentication agent.\n",
        );
        assert!(!none.running);
        assert!(none.problem.unwrap().contains("Could not open"));
    }

    #[test]
    fn names_agents_by_their_socket() {
        assert_eq!(
            agent_kind(Some(
                "/Users/r/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock"
            )),
            AgentKind::OnePassword
        );
        assert_eq!(
            agent_kind(Some("/run/user/1000/gnupg/S.gpg-agent.ssh")),
            AgentKind::GnuPg
        );
        assert_eq!(
            agent_kind(Some("/private/tmp/com.apple.launchd.x/Listeners")),
            AgentKind::MacOs
        );
        assert_eq!(
            agent_kind(Some("/tmp/ssh-XXXX/agent.123")),
            AgentKind::SshAgent
        );
        assert_eq!(
            agent_kind(Some(r"\\.\pipe\openssh-ssh-agent")),
            AgentKind::WindowsOpenSsh
        );
    }

    #[test]
    fn reads_what_ssh_would_use_for_a_host() {
        let c = parse_host_config(
            "user git\nhostname ssh.github.com\nport 443\nidentityfile ~/.ssh/id_ed25519\nidentityfile ~/.ssh/work\nidentitiesonly yes\nidentityagent /tmp/agent.sock\naddkeystoagent false\nstricthostkeychecking ask\n",
        );
        assert_eq!(
            (c.hostname.as_str(), c.user.as_str(), c.port),
            ("ssh.github.com", "git", 443)
        );
        assert_eq!(c.identity_files, ["~/.ssh/id_ed25519", "~/.ssh/work"]);
        assert!(c.identities_only);
        assert_eq!(c.identity_agent.as_deref(), Some("/tmp/agent.sock"));
    }

    #[test]
    fn classifies_connection_tests() {
        let c = |code, text| classify(Some(code), text);
        assert_eq!(
            c(
                1,
                "Hi rocky! You've successfully authenticated, but GitHub does not provide shell access."
            ),
            Connection::SignedIn(Some("rocky".into()))
        );
        assert_eq!(
            c(0, "Welcome to GitLab, @rocky-raccoon!"),
            Connection::SignedIn(Some("rocky-raccoon".into()))
        );
        assert_eq!(
            c(
                0,
                "authenticated via ssh key.\n\nYou can use git to connect to Bitbucket. Shell access is disabled\nThis deploy key... logged in as rocky."
            ),
            Connection::SignedIn(Some("rocky".into()))
        );
        assert_eq!(
            c(
                1,
                "Hi there, rocky! You've successfully authenticated with the key named laptop, but Forgejo does not provide shell access."
            ),
            Connection::SignedIn(Some("rocky".into()))
        );
        assert_eq!(
            c(
                255,
                "debug1: Authentications that can continue: publickey\ndebug1: No more authentication methods to try.\ngit@github.com: Permission denied (publickey)."
            ),
            Connection::NoKey
        );
        assert_eq!(
            c(
                255,
                "debug1: Offering public key: /home/r/.ssh/id_ed25519 ED25519 SHA256:abc agent\ndebug1: Authentications that can continue: publickey\ngit@github.com: Permission denied (publickey)."
            ),
            Connection::KeyRefused(vec!["SHA256:abc".into()])
        );
        assert_eq!(
            c(
                255,
                "@@@@@@@@@@@\n@    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!     @\n@@@@@@@@@@@\nHost key verification failed."
            ),
            Connection::HostKeyChanged
        );
        assert_eq!(
            c(
                255,
                "No ED25519 host key is known for github.com and you have requested strict checking.\nHost key verification failed."
            ),
            Connection::HostUnknown
        );
        assert_eq!(
            c(
                255,
                "ssh: connect to host github.com port 22: Connection timed out"
            ),
            Connection::PortBlocked
        );
        assert_eq!(
            c(
                255,
                "kex_exchange_identification: Connection closed by remote host"
            ),
            Connection::PortBlocked
        );
        assert_eq!(
            c(
                255,
                "ssh: Could not resolve hostname githb.com: Name or service not known"
            ),
            Connection::NameNotFound
        );
        assert_eq!(
            c(
                255,
                "debug1: Reading configuration data\nsomething odd happened\ndebug1: Exit status 255"
            ),
            Connection::Other("something odd happened".into())
        );
    }

    /// An OpenSSH key, made the way `ssh-keygen` would.
    fn write_key(dir: &Path, name: &str, comment: &str, passphrase: Option<&str>) {
        use ssh_key::rand_core::OsRng;
        let mut key = PrivateKey::random(&mut OsRng, ssh_key::Algorithm::Ed25519).unwrap();
        key.set_comment(comment);
        let public = key.public_key().to_openssh().unwrap();
        let key = match passphrase {
            Some(p) => key.encrypt(&mut OsRng, p).unwrap(),
            None => key,
        };
        let text = key.to_openssh(ssh_key::LineEnding::LF).unwrap();
        std::fs::write(dir.join(name), text.as_bytes()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(dir.join(name), std::fs::Permissions::from_mode(0o600))
                .unwrap();
        }
        std::fs::write(dir.join(format!("{name}.pub")), format!("{public}\n")).unwrap();
    }

    #[test]
    fn finds_keys_with_their_fingerprints_and_passphrases() {
        let t = crate::testutil::TempRepo::new();
        let home = t.dir.join("home");
        let ssh = home.join(".ssh");
        let elsewhere = home.join("keys");
        std::fs::create_dir_all(&ssh).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        write_key(&ssh, "id_ed25519", "rocky@box", None);
        write_key(&ssh, "work", "work key", Some("secret phrase"));
        write_key(&elsewhere, "deploy", "deploy key", None);
        std::fs::write(ssh.join("known_hosts"), "github.com ssh-ed25519 AAAA\n").unwrap();
        std::fs::write(
            ssh.join("config"),
            "Host github.com\n  IdentityFile ~/keys/deploy\nHost *\n  IdentityFile=~/.ssh/work\n",
        )
        .unwrap();
        let found = keys(&ssh);
        let names: Vec<String> = found
            .iter()
            .map(|k| k.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        // Those in ~/.ssh first, then ones the config names from elsewhere.
        assert_eq!(names, ["id_ed25519", "work", "deploy"]);
        let by = |n: &str| found.iter().find(|k| k.path.ends_with(n)).unwrap();
        assert_eq!(by("id_ed25519").passphrase, Some(false));
        assert_eq!(by("work").passphrase, Some(true));
        assert_eq!(by("work").comment, "work key");
        assert!(by("work").in_config && by("deploy").in_config && !by("id_ed25519").in_config);
        assert_eq!(by("id_ed25519").kind, "ED25519");
        let fp = by("id_ed25519").fingerprint.clone().unwrap();
        assert!(fp.starts_with("SHA256:"), "{fp}");
        // The same fingerprint as ssh-keygen gives, where there is one.
        if let Ok(out) = Command::new("ssh-keygen")
            .args(["-l", "-E", "sha256", "-f"])
            .arg(ssh.join("id_ed25519.pub"))
            .output()
            && out.status.success()
        {
            assert!(String::from_utf8_lossy(&out.stdout).contains(&fp));
        }
    }

    /// A stand-in `ssh` that prints what it's given to say, and exits so.
    #[cfg(unix)]
    fn fake_ssh(dir: &Path, stderr: &str, code: i32) -> Ssh {
        use std::os::unix::fs::PermissionsExt as _;
        let path = dir.join("ssh");
        let said = dir.join("said.txt");
        std::fs::write(&said, stderr).unwrap();
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\necho \"$@\" > '{}'\ncat '{}' >&2\nexit {code}\n",
                dir.join("args.txt").display(),
                said.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        Ssh::from_program(&path.to_string_lossy(), "ssh")
    }

    #[cfg(unix)]
    #[test]
    fn tests_a_connection_through_the_users_ssh() {
        let t = crate::testutil::TempRepo::new();
        let ssh = fake_ssh(
            &t.dir,
            "Hi rocky! You've successfully authenticated, but GitHub does not provide shell access.\n",
            1,
        );
        assert_eq!(
            test_connection(&ssh, "git", "github.com", Some(443)).unwrap(),
            Connection::SignedIn(Some("rocky".into()))
        );
        let args = std::fs::read_to_string(t.dir.join("args.txt")).unwrap();
        assert!(
            args.contains("-T")
                && args.contains("-p 443")
                && args.trim_end().ends_with("git@github.com"),
            "{args}"
        );
        let refused = fake_ssh(
            &t.dir,
            "git@github.com: Permission denied (publickey).\n",
            255,
        );
        assert_eq!(
            test_connection(&refused, "git", "github.com", None).unwrap(),
            Connection::NoKey
        );
    }

    /// A real `ssh-agent` (where there is one): empty, then with a key.
    #[cfg(unix)]
    #[test]
    fn lists_a_real_agents_keys() {
        let t = crate::testutil::TempRepo::new();
        let socket = t.dir.join("agent.sock");
        let Ok(mut agent_process) = Command::new("ssh-agent")
            .args(["-D", "-a"])
            .arg(&socket)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return;
        };
        let start = std::time::Instant::now();
        while !socket.exists() && start.elapsed() < std::time::Duration::from_secs(5) {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let ssh = Ssh::from_program("ssh", "ssh");
        let at = Some(socket.to_string_lossy().into_owned());
        let empty = agent_at(&ssh, at.clone());
        assert!(empty.running && empty.keys.is_empty(), "{empty:?}");
        write_key(&t.dir, "k", "rocky@box", None);
        let added = Command::new("ssh-add")
            .arg(t.dir.join("k"))
            .env("SSH_AUTH_SOCK", &socket)
            .output()
            .unwrap();
        assert!(added.status.success(), "{added:?}");
        let full = agent_at(&ssh, at);
        assert_eq!(full.keys.len(), 1);
        assert_eq!(
            Some(full.keys[0].fingerprint.clone()),
            read_key(&t.dir.join("k")).fingerprint
        );
        assert_eq!(full.keys[0].comment, "rocky@box");
        let _ = agent_process.kill();
        // None there at all.
        let gone = agent_at(
            &ssh,
            Some(t.dir.join("nothing.sock").to_string_lossy().into_owned()),
        );
        assert!(!gone.running);
    }

    #[test]
    fn uses_the_ssh_git_would() {
        let t = crate::testutil::TempRepo::new();
        crate::cli::run(
            t.path(),
            &["config", "core.sshCommand", "ssh -i ~/.ssh/work"],
            None,
        )
        .unwrap();
        if env("GIT_SSH_COMMAND").is_none() {
            let s = Ssh::for_git(Some(t.path()));
            assert_eq!(s.from, "core.sshCommand");
            assert_eq!(s.command, ["ssh", "-i", "~/.ssh/work"]);
        }
    }
}
