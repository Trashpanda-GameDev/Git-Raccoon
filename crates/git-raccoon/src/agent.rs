//! Letting AI agents (Claude Code, Claude Desktop, Cursor…) work with Git
//! Raccoon, over the Model Context Protocol (MCP).
//!
//! An agent runs `git-raccoon mcp`: that process ([`run_bridge`]) relays MCP
//! messages between the agent (on its standard input and output) and the
//! running app. The app only listens while agent access is turned on, only
//! on 127.0.0.1, and each connection must first give a random token. Where
//! to connect, and the token, are in a file beside the settings that only
//! this user can read ([`connection_file`]).
//!
//! The app sees each tool call as a [`Call`] and answers it: reads straight
//! away, changes once the person allows them.

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// Overrides where the connection file is (tests use their own).
pub const FILE_ENV: &str = "GIT_RACCOON_AGENT_FILE";

/// MCP versions this speaks, newest first.
const VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// Where the app says how to reach it: beside the settings.
pub fn connection_file() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(FILE_ENV) {
        return Some(PathBuf::from(path));
    }
    eframe::storage_dir("git-raccoon").map(|d| d.join("agent.json"))
}

/// A tool call from an agent, waiting for its answer.
pub struct Call {
    /// The agent's name, as it gave it ("claude-code").
    pub client: String,
    /// Its connection: "allow for this session" holds for this one.
    pub connection: u64,
    pub tool: String,
    pub args: Value,
    reply: Sender<Answer>,
}

/// What a tool call came to: its text, and whether it's an error.
pub struct Answer {
    pub text: String,
    pub error: bool,
}

impl Call {
    pub fn ok(self, text: impl Into<String>) {
        let _ = self.reply.send(Answer {
            text: text.into(),
            error: false,
        });
    }

    pub fn fail(self, text: impl Into<String>) {
        let _ = self.reply.send(Answer {
            text: text.into(),
            error: true,
        });
    }

    /// Answers from another thread when it's done (reads take a while).
    pub fn answer(self, result: Result<String, String>) {
        match result {
            Ok(text) => self.ok(text),
            Err(e) => self.fail(e),
        }
    }

    /// A string argument, if given (and not blank).
    pub fn str(&self, name: &str) -> Option<String> {
        self.args
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        self.args.get(name).and_then(Value::as_bool)
    }

    pub fn number(&self, name: &str) -> Option<u64> {
        self.args.get(name).and_then(Value::as_u64)
    }

    /// A list of strings (a single string counts as one).
    pub fn strings(&self, name: &str) -> Vec<String> {
        match self.args.get(name) {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            Some(Value::String(s)) => vec![s.clone()],
            _ => Vec::new(),
        }
    }
}

/// An agent connected now.
#[derive(Debug, Clone, PartialEq)]
pub struct Client {
    pub connection: u64,
    pub name: String,
}

/// Lets agents in for as long as it lives.
pub struct Server {
    calls: Receiver<Call>,
    clients: Arc<Mutex<Vec<Client>>>,
    stop: Arc<AtomicBool>,
    port: u16,
    token: String,
    file: Option<PathBuf>,
}

impl Server {
    /// Starts listening, and says where in `file`. `wake` is called when a
    /// call arrives.
    pub fn start(
        file: Option<PathBuf>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> std::io::Result<Server> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        let token = git_raccoon_core::askpass::random_token();
        if let Some(file) = &file {
            write_private(
                file,
                &json!({ "port": port, "token": token, "pid": std::process::id() }).to_string(),
            )?;
        }
        let (send, calls) = mpsc::channel();
        let clients = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        let next = Arc::new(AtomicU64::new(1));
        {
            let (token, clients, stop) = (token.clone(), clients.clone(), stop.clone());
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let connection = next.fetch_add(1, Ordering::Relaxed);
                    let (token, send, clients, wake, stop) = (
                        token.clone(),
                        send.clone(),
                        clients.clone(),
                        wake.clone(),
                        stop.clone(),
                    );
                    std::thread::spawn(move || {
                        let link = Link {
                            connection,
                            send,
                            clients: clients.clone(),
                            wake,
                            stop,
                        };
                        let _ = serve(stream, &token, &link);
                        let mut list = clients.lock().unwrap_or_else(|e| e.into_inner());
                        list.retain(|c| c.connection != connection);
                        (link.wake)();
                    });
                }
            });
        }
        Ok(Server {
            calls,
            clients,
            stop,
            port,
            token,
            file,
        })
    }

    pub fn poll(&self) -> Option<Call> {
        self.calls.try_recv().ok()
    }

    /// The agents connected now.
    pub fn clients(&self) -> Vec<Client> {
        self.clients
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Where it listens, and the token (for tests).
    #[doc(hidden)]
    pub fn address(&self) -> (u16, String) {
        (self.port, self.token.clone())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Wake the listener so it notices.
        let _ = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port));
        // The file is ours unless another copy of the app wrote its own since.
        if let Some(file) = &self.file
            && std::fs::read_to_string(file).is_ok_and(|text| text.contains(&self.token))
        {
            let _ = std::fs::remove_file(file);
        }
    }
}

/// Writes `text` to `file`, readable by this user only.
fn write_private(file: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(file);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    // (On Windows, the settings folder is the user's own.)
    options.open(file)?.write_all(text.as_bytes())
}

/// What a connection's thread shares with the app.
struct Link {
    connection: u64,
    send: Sender<Call>,
    clients: Arc<Mutex<Vec<Client>>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    stop: Arc<AtomicBool>,
}

/// One agent's connection: its token, then MCP messages, one per line.
fn serve(stream: TcpStream, token: &str, link: &Link) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    let mut first = String::new();
    reader.read_line(&mut first)?;
    if first.trim_end() != token {
        return Ok(());
    }
    let mut name = "An agent".to_string();
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || link.stop.load(Ordering::Relaxed) {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle(&message, &mut name, link),
            Err(_) => Some(error(Value::Null, -32700, "That isn't JSON.")),
        };
        if let Some(reply) = reply {
            writer.write_all(format!("{reply}\n").as_bytes())?;
            writer.flush()?;
        }
    }
    let _ = writer.shutdown(Shutdown::Both);
    Ok(())
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// Answers one message; `None` for notifications (and replies to us).
fn handle(message: &Value, name: &mut String, link: &Link) -> Option<Value> {
    let id = message.get("id")?.clone();
    let method = message.get("method").and_then(Value::as_str)?;
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => {
            if let Some(n) = params
                .pointer("/clientInfo/name")
                .and_then(Value::as_str)
                .filter(|n| !n.trim().is_empty())
            {
                *name = n.trim().to_string();
            }
            let mut list = link.clients.lock().unwrap_or_else(|e| e.into_inner());
            list.retain(|c| c.connection != link.connection);
            list.push(Client {
                connection: link.connection,
                name: name.clone(),
            });
            drop(list);
            (link.wake)();
            let asked = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let version = VERSIONS
                .iter()
                .find(|v| **v == asked)
                .unwrap_or(&VERSIONS[0]);
            result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "git-raccoon", "title": "Git Raccoon", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => result(id, json!({})),
        "tools/list" => result(id, json!({ "tools": tools() })),
        "tools/call" => {
            let Some(tool) = params.get("name").and_then(Value::as_str) else {
                return Some(error(id, -32602, "Which tool? (No name given.)"));
            };
            if !tools()
                .as_array()
                .is_some_and(|all| all.iter().any(|t| t["name"] == tool))
            {
                return Some(error(id, -32602, &format!("There's no tool named {tool}.")));
            }
            let (reply, answer) = mpsc::channel();
            let call = Call {
                client: name.clone(),
                connection: link.connection,
                tool: tool.to_string(),
                args: params.get("arguments").cloned().unwrap_or(json!({})),
                reply,
            };
            if link.send.send(call).is_err() {
                return Some(error(
                    id,
                    -32000,
                    "Git Raccoon has stopped letting agents in.",
                ));
            }
            (link.wake)();
            let answer = answer.recv().unwrap_or(Answer {
                text: "Git Raccoon stopped letting agents in before it answered.".into(),
                error: true,
            });
            result(
                id,
                json!({
                    "content": [{ "type": "text", "text": answer.text }],
                    "isError": answer.error,
                }),
            )
        }
        _ => error(
            id,
            -32601,
            &format!("Git Raccoon doesn't know \"{method}\"."),
        ),
    })
}

/// What the agent is told about working with Git Raccoon.
const INSTRUCTIONS: &str = "Git Raccoon is a Git client open on this computer, with the person watching. \
Reading (status, history, diffs, branches, pull/merge requests) is free. Anything that changes the \
repository asks the person first, so a change can be refused: say what you'd like to do and why. \
Tools act on the repository in front unless given `repository` (a path from list_repositories). \
get_view says what the person is looking at. Comments on a pull/merge request are added as drafts \
that the person sends when they finish their review.";

/// The tools: reads, ways to show things, and changes (which ask first).
pub fn tools() -> Value {
    let repo = json!({ "type": "string", "description": "The repository's path (from list_repositories); the one in front if left out." });
    let read = json!({ "readOnlyHint": true, "openWorldHint": false });
    let change = json!({ "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false });
    let remote = json!({ "readOnlyHint": false, "destructiveHint": false, "openWorldHint": true });
    let tool =
        |name: &str, title: &str, about: &str, props: Value, required: &[&str], notes: &Value| {
            json!({
                "name": name,
                "title": title,
                "description": about,
                "inputSchema": { "type": "object", "properties": props, "required": required },
                "annotations": notes,
            })
        };
    json!([
        // Reading.
        tool(
            "list_repositories",
            "List open repositories",
            "The repositories open in Git Raccoon's tabs, and which one is in front.",
            json!({}),
            &[],
            &read
        ),
        tool(
            "get_view",
            "What the person is looking at",
            "What's on screen in Git Raccoon: the repository in front, the selected commit, the open file's diff, and the pull/merge request or issue open.",
            json!({}),
            &[],
            &read
        ),
        tool(
            "get_status",
            "Working changes",
            "The branch checked out, how far it is ahead of or behind its upstream, the staged, unstaged and conflicted files, and any merge or rebase in progress.",
            json!({ "repository": repo }),
            &[],
            &read
        ),
        tool(
            "get_log",
            "History",
            "Commits, newest first: of a branch, tag or commit (default: what's checked out), and only those changing a path if given.",
            json!({
                "repository": repo,
                "revision": { "type": "string", "description": "A branch, tag or commit; or \"--all\" for every branch." },
                "path": { "type": "string", "description": "Only commits that change this file or folder." },
                "limit": { "type": "integer", "description": "How many (default 30, at most 500)." },
                "skip": { "type": "integer", "description": "How many to skip first, to page." },
            }),
            &[],
            &read
        ),
        tool(
            "search_commits",
            "Search commits",
            "Finds commits on every branch whose message or author contains the text, or whose changes add or remove it.",
            json!({
                "repository": repo,
                "query": { "type": "string" },
                "in": { "type": "string", "enum": ["message", "author", "changes"], "description": "Where to look (default: message)." },
                "limit": { "type": "integer", "description": "How many (default 30, at most 200)." },
            }),
            &["query"],
            &read
        ),
        tool(
            "get_commit",
            "Commit details",
            "A commit's message, author, parents and the files it changes (with lines added and removed).",
            json!({ "repository": repo, "id": { "type": "string", "description": "A commit id (or branch or tag)." } }),
            &["id"],
            &read
        ),
        tool(
            "get_diff",
            "Diff",
            "A unified diff: of a commit (with `commit`), of the staged changes (`staged`: true) or of the unstaged ones (the default); of one file with `path`.",
            json!({
                "repository": repo,
                "commit": { "type": "string" },
                "staged": { "type": "boolean" },
                "path": { "type": "string" },
            }),
            &[],
            &read
        ),
        tool(
            "get_refs",
            "Branches, tags, stashes and remotes",
            "Local branches (with their upstream, ahead and behind), remotes and their branches, tags, and stashes.",
            json!({ "repository": repo }),
            &[],
            &read
        ),
        tool(
            "list_requests",
            "Pull/merge requests and issues",
            "The open pull (GitHub) or merge (GitLab) requests and issues of the repository's project, as Git Raccoon lists them.",
            json!({ "repository": repo }),
            &[],
            &read
        ),
        tool(
            "get_request",
            "The open pull/merge request",
            "The request open in Git Raccoon (show_request opens one): its description, state, checks, review threads (with ids, files and lines), conversation, files and commits, and your draft comments.",
            json!({}),
            &[],
            &read
        ),
        // Showing.
        tool(
            "show_commit",
            "Show a commit",
            "Selects a commit in Git Raccoon's history, so the person sees it.",
            json!({ "repository": repo, "id": { "type": "string" } }),
            &["id"],
            &read
        ),
        tool(
            "show_request",
            "Open a pull/merge request",
            "Opens a pull/merge request in Git Raccoon by its number (then get_request reads it).",
            json!({ "repository": repo, "number": { "type": "integer" } }),
            &["number"],
            &read
        ),
        tool(
            "open_repository",
            "Open a repository",
            "Opens a repository on this computer in a tab of Git Raccoon (or brings its tab to the front).",
            json!({ "path": { "type": "string" } }),
            &["path"],
            &read
        ),
        tool(
            "start_request",
            "Start a pull/merge request",
            "Opens Git Raccoon's new pull/merge request view for the branch checked out, filled in with a title and description, for the person to check and open.",
            json!({
                "repository": repo,
                "title": { "type": "string" },
                "body": { "type": "string", "description": "The description, in Markdown." },
                "target": { "type": "string", "description": "The branch it goes into (default: the project's default)." },
            }),
            &[],
            &read
        ),
        // Conflicts.
        tool(
            "get_conflicts",
            "Conflicts",
            "The merge, rebase, cherry-pick or revert in progress, its two sides named by branch (\"ours\" is what the files had, \"theirs\" what's coming in), and each conflicted file: how it conflicts, and for text files each conflict's base (what both started from) and both sides, and the file's text with Git's markers. `path` for one file.",
            json!({ "repository": repo, "path": { "type": "string" } }),
            &[],
            &read
        ),
        tool(
            "propose_resolution",
            "Propose a conflict's resolution",
            "Shows the person a resolution for a conflicted file: its whole text, without conflict markers, and your reasoning (which side's change you kept, and why). Nothing is written unless they choose to use it; they may edit it first.",
            json!({
                "repository": repo,
                "path": { "type": "string" },
                "text": { "type": "string", "description": "The whole file as resolved." },
                "reasoning": { "type": "string", "description": "A few plain sentences: what each side did, and what you kept." },
            }),
            &["path", "text", "reasoning"],
            &read
        ),
        // Changing (each asks the person first).
        tool(
            "stage",
            "Stage changes",
            "Stages files' changes (or all of them). Asks the person first.",
            json!({ "repository": repo, "paths": { "type": "array", "items": { "type": "string" } }, "all": { "type": "boolean" } }),
            &[],
            &change
        ),
        tool(
            "unstage",
            "Unstage changes",
            "Unstages files' changes (or all of them). Asks the person first.",
            json!({ "repository": repo, "paths": { "type": "array", "items": { "type": "string" } }, "all": { "type": "boolean" } }),
            &[],
            &change
        ),
        tool(
            "commit",
            "Commit",
            "Commits the staged changes with a message (first line: the summary). `amend` replaces the last commit instead. Asks the person first.",
            json!({ "repository": repo, "message": { "type": "string" }, "amend": { "type": "boolean" } }),
            &["message"],
            &change
        ),
        tool(
            "create_branch",
            "Create a branch",
            "Creates a branch at a commit (default: what's checked out), and checks it out unless `checkout` is false. Asks the person first.",
            json!({ "repository": repo, "name": { "type": "string" }, "start": { "type": "string" }, "checkout": { "type": "boolean" } }),
            &["name"],
            &change
        ),
        tool(
            "checkout",
            "Check out a branch",
            "Checks out a local branch, or a remote one (\"origin/feature\") through a local branch that tracks it. Asks the person first.",
            json!({ "repository": repo, "branch": { "type": "string" } }),
            &["branch"],
            &change
        ),
        tool(
            "stash",
            "Stash changes",
            "Puts the uncommitted changes aside in a stash. Asks the person first.",
            json!({ "repository": repo, "message": { "type": "string" }, "include_untracked": { "type": "boolean" } }),
            &[],
            &change
        ),
        tool(
            "stash_pop",
            "Pop a stash",
            "Applies a stash (by its index from get_refs, default 0) and drops it. Asks the person first.",
            json!({ "repository": repo, "index": { "type": "integer" } }),
            &[],
            &change
        ),
        tool(
            "create_tag",
            "Create a tag",
            "Creates a tag at a commit (default: what's checked out); with a message, an annotated one. Asks the person first.",
            json!({ "repository": repo, "name": { "type": "string" }, "target": { "type": "string" }, "message": { "type": "string" } }),
            &["name"],
            &change
        ),
        tool(
            "merge",
            "Merge a branch",
            "Merges a branch (local, or remote like \"origin/main\") into the one checked out. Asks the person first.",
            json!({ "repository": repo, "branch": { "type": "string" } }),
            &["branch"],
            &change
        ),
        tool(
            "fetch",
            "Fetch",
            "Fetches from every remote. Asks the person first.",
            json!({ "repository": repo }),
            &[],
            &remote
        ),
        tool(
            "pull",
            "Pull",
            "Pulls the branch checked out from its upstream, the way the person's settings say. Asks the person first.",
            json!({ "repository": repo }),
            &[],
            &remote
        ),
        tool(
            "push",
            "Push",
            "Pushes the branch checked out (to its upstream; a new branch goes to origin under its name). Never forces. Asks the person first.",
            json!({ "repository": repo }),
            &[],
            &remote
        ),
        tool(
            "discard",
            "Discard changes",
            "Throws away files' unstaged changes (their current versions go to the system's trash). Always asks the person.",
            json!({ "repository": repo, "paths": { "type": "array", "items": { "type": "string" } } }),
            &["paths"],
            &json!({ "readOnlyHint": false, "destructiveHint": true, "openWorldHint": false })
        ),
        // The open pull/merge request.
        tool(
            "draft_comment",
            "Draft a comment",
            "Adds a draft comment to the open pull/merge request: on the whole request, on a line of a file (`path` and `line`, of the new version unless `side` is \"old\"), or as a reply to a thread (`reply_to`: its id from get_request). The person sends it when they finish their review. Asks the person first.",
            json!({
                "body": { "type": "string", "description": "Markdown." },
                "path": { "type": "string" },
                "line": { "type": "integer" },
                "side": { "type": "string", "enum": ["new", "old"] },
                "reply_to": { "type": "string" },
            }),
            &["body"],
            &change
        ),
        tool(
            "resolve_thread",
            "Resolve a thread",
            "Resolves (or, with `resolved`: false, reopens) a review thread of the open pull/merge request, on the forge. Asks the person first.",
            json!({ "thread": { "type": "string", "description": "Its id, from get_request." }, "resolved": { "type": "boolean" } }),
            &["thread"],
            &remote
        ),
    ])
}

/// What `git-raccoon mcp` does: relays MCP between the agent (standard
/// input and output) and the running app. Returns the exit code.
pub fn run_bridge() -> i32 {
    let Some(file) = connection_file() else {
        return refuse("Git Raccoon couldn't find its settings folder.");
    };
    // The app may be starting: give it a moment (tests, less).
    let wait = std::env::var("GIT_RACCOON_AGENT_WAIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let deadline = Instant::now() + Duration::from_secs(wait);
    let stream = loop {
        match connect(&file) {
            Ok(stream) => break stream,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(250)),
            Err(_) => {
                return refuse(
                    "Git Raccoon isn't letting agents in. Open it and turn on Settings › Agents › Agent access, then try again.",
                );
            }
        }
    };
    let Ok(mut upstream) = stream.try_clone() else {
        return 1;
    };
    // From the app to the agent.
    let down = std::thread::spawn(move || {
        let mut out = std::io::stdout();
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
                break;
            }
        }
    });
    // From the agent to the app.
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if writeln!(upstream, "{line}").is_err() {
            break;
        }
    }
    let _ = upstream.shutdown(Shutdown::Write);
    let _ = down.join();
    0
}

/// Connects to the app the file names, and gives the token.
fn connect(file: &Path) -> std::io::Result<TcpStream> {
    let text = std::fs::read_to_string(file)?;
    let info: Value = serde_json::from_str(&text).map_err(std::io::Error::other)?;
    let port = info["port"]
        .as_u64()
        .and_then(|p| u16::try_from(p).ok())
        .ok_or_else(|| std::io::Error::other("no port"))?;
    let token = info["token"]
        .as_str()
        .ok_or_else(|| std::io::Error::other("no token"))?;
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))?;
    stream.write_all(format!("{token}\n").as_bytes())?;
    Ok(stream)
}

/// Answers every request with why there's no Git Raccoon to talk to, until
/// the agent hangs up.
fn refuse(why: &str) -> i32 {
    let mut out = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(id) = message
            .get("id")
            .filter(|_| message.get("method").is_some())
        {
            let reply = error(id.clone(), -32000, why);
            if writeln!(out, "{reply}").and_then(|()| out.flush()).is_err() {
                break;
            }
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send(stream: &mut TcpStream, message: &Value) -> Value {
        stream.write_all(format!("{message}\n").as_bytes()).unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        serde_json::from_str(&line).unwrap()
    }

    #[test]
    fn speaks_mcp_and_passes_tool_calls_to_the_app() {
        let dir = std::env::temp_dir().join(format!("git-raccoon-agent-{}", std::process::id()));
        let file = dir.join("agent.json");
        let server = Server::start(Some(file.clone()), || {}).unwrap();
        // The file says where, and only to this user.
        let info: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        let (port, token) = server.address();
        assert_eq!(info["port"], port);
        assert_eq!(info["token"], token);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        let mut stream = connect(&file).unwrap();
        let init = send(
            &mut stream,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2025-03-26", "clientInfo": { "name": "test-agent" }, "capabilities": {} } }),
        );
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "git-raccoon");
        assert_eq!(server.clients()[0].name, "test-agent");

        let list = send(
            &mut stream,
            &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        );
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(
            names.contains(&"get_status") && names.contains(&"commit"),
            "{names:?}"
        );

        // A call goes to the app, and its answer back.
        let app = std::thread::spawn(move || {
            let call = loop {
                if let Some(call) = server.poll() {
                    break call;
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            assert_eq!(call.client, "test-agent");
            assert_eq!(call.str("repository").as_deref(), Some("/x"));
            call.ok("all clean");
            server
        });
        let answer = send(
            &mut stream,
            &json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": { "name": "get_status", "arguments": { "repository": "/x" } } }),
        );
        assert_eq!(answer["result"]["content"][0]["text"], "all clean");
        assert_eq!(answer["result"]["isError"], false);
        let server = app.join().unwrap();

        // Unknown tools and methods are errors; notifications get no reply.
        let unknown = send(
            &mut stream,
            &json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "rm_rf" } }),
        );
        assert_eq!(unknown["error"]["code"], -32602);
        stream
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .unwrap();
        let ping = send(
            &mut stream,
            &json!({ "jsonrpc": "2.0", "id": 5, "method": "ping" }),
        );
        assert_eq!(ping["id"], 5);

        // A wrong token gets nowhere.
        let mut other = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        other.write_all(b"wrong\n").unwrap();
        other
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n")
            .unwrap();
        let mut line = String::new();
        let read = BufReader::new(other).read_line(&mut line).unwrap_or(0);
        assert_eq!(read, 0, "closed without an answer: {line}");

        // Stopping removes the file.
        drop(server);
        assert!(!file.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
