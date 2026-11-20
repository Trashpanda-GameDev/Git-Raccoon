//! `git-raccoon mcp`, as an agent starts it: relaying MCP to the app, or
//! saying why it can't.

#![expect(
    clippy::unwrap_used,
    reason = "test setup: a failure should simply fail the test"
)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::process::{Command, Stdio};
use std::time::Duration;

use git_raccoon::agent::{FILE_ENV, Server};
use serde_json::{Value, json};

fn bridge(file: &std::path::Path) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_git-raccoon"))
        .arg("mcp")
        .env(FILE_ENV, file)
        .env("GIT_RACCOON_AGENT_WAIT", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
}

#[test]
fn relays_between_the_agent_and_the_app() {
    let dir = std::env::temp_dir().join(format!("git-raccoon-bridge-{}", std::process::id()));
    let file = dir.join("agent.json");
    let server = Server::start(Some(file.clone()), || {}).unwrap();
    let mut child = bridge(&file);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut ask = |message: Value| {
        writeln!(stdin, "{message}").unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str::<Value>(&line).unwrap()
    };
    let init = ask(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2025-06-18", "clientInfo": { "name": "bridge-test" }, "capabilities": {} } }));
    assert_eq!(init["result"]["serverInfo"]["name"], "git-raccoon");

    let app = std::thread::spawn(move || {
        loop {
            if let Some(call) = server.poll() {
                assert_eq!(call.tool, "get_view");
                call.ok("looking at main");
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        server
    });
    let answer = ask(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": { "name": "get_view", "arguments": {} } }));
    assert_eq!(answer["result"]["content"][0]["text"], "looking at main");
    let server = app.join().unwrap();

    // The agent hangs up: the bridge finishes.
    drop(stdin);
    assert!(child.wait().unwrap().success());
    drop(server);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn says_why_when_the_app_isnt_letting_agents_in() {
    let file =
        std::env::temp_dir().join(format!("git-raccoon-no-agent-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&file);
    let mut child = bridge(&file);
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        "{}",
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} })
    )
    .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let answer: Value = serde_json::from_str(&line).unwrap();
    let message = answer["error"]["message"].as_str().unwrap();
    assert!(message.contains("Settings › Agents"), "{message}");
    drop(stdin);
    assert!(!child.wait().unwrap().success());
}
