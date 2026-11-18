//! Git asking for a username and password, answered in the app.
//!
//! A tiny HTTP server stands in for a Git host that wants a sign-in, and
//! the app's real executable answers Git's questions. In its own test file
//! (its own process) because it adjusts the environment first.

#![expect(
    clippy::unwrap_used,
    reason = "test setup: a failure should simply fail the test"
)]

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use git_raccoon::App;
use git_raccoon::forge::{Account, Kind, accounts};
use git_raccoon::settings::Settings;

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Answers every request with 401 (asking for a sign-in) until one carries
/// credentials; records those and refuses them (403).
fn sign_in_server() -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&buf[..n]),
                }
            }
            let text = String::from_utf8_lossy(&request).to_string();
            let auth = text
                .lines()
                .find_map(|l| l.strip_prefix("Authorization: Basic "))
                .map(|a| a.trim().to_string());
            let reply = match auth {
                Some(a) => {
                    record.lock().unwrap().push(a);
                    "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                }
                None => {
                    "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"test\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                }
            };
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    (port, seen)
}

fn wait_for(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let start = Instant::now();
    while !done(h.state()) {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "timed out waiting for {what}"
        );
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    h.run_ok();
}

fn question(app: &App) -> Option<String> {
    app.git_question().map(str::to_string)
}

#[test]
fn asks_for_a_username_and_password_in_the_app() {
    // Some environments tell Git never to ask (credential.interactive=false
    // through GIT_CONFIG_*); a user's machine wouldn't. Nothing else runs in
    // this process yet, so changing the environment is safe.
    #[expect(unsafe_code, reason = "single-threaded at this point; see above")]
    unsafe {
        std::env::set_var("GIT_CONFIG_COUNT", "0");
    }

    let dir = std::env::temp_dir().join(format!(
        "git-raccoon-askpass-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(
        &dir,
        &[
            "-c",
            "user.name=Rocky",
            "-c",
            "user.email=r@example.com",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "first",
        ],
    );
    let (port, seen) = sign_in_server();
    git(
        &dir,
        &[
            "remote",
            "add",
            "origin",
            &format!("http://127.0.0.1:{port}/repo.git"),
        ],
    );
    // Straight to the test server: no proxy, no stored credentials.
    git(&dir, &["config", "remote.origin.proxy", ""]);
    git(&dir, &["config", "credential.helper", ""]);

    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .wgpu()
        .build_eframe(|cc| {
            let mut app = App::with_settings(&cc.egui_ctx, Settings::default());
            app.answer_git_questions(&cc.egui_ctx, Path::new(env!("CARGO_BIN_EXE_git-raccoon")));
            app
        });
    let ctx = h.ctx.clone();
    h.state_mut().open(&ctx, dir.to_str().unwrap());
    wait_for(&mut h, "the repo", |app| {
        app.repo().is_some() && !app.is_busy()
    });

    // Fetch: Git asks for a username, then a password, in the app.
    h.get_by_label("Fetch").click();
    wait_for(&mut h, "the username question", |app| {
        question(app).is_some_and(|q| q.contains("Username"))
    });
    h.get_by_label("Answer").click();
    h.run_ok();
    h.get_by_label("Answer").type_text("rocky");
    h.run_ok();
    h.get_by_label("OK").click();
    wait_for(&mut h, "the password question", |app| {
        question(app).is_some_and(|q| q.contains("Password"))
    });
    h.get_by_label("Secret answer").click();
    h.run_ok();
    h.get_by_label("Secret answer").type_text("hunter2");
    h.run_ok();
    if let Ok(shots) = std::env::var("GIT_RACCOON_SCREENSHOTS") {
        h.render()
            .unwrap()
            .save(Path::new(&shots).join("24-sign-in.png"))
            .unwrap();
    }
    h.get_by_label("OK").click();
    wait_for(&mut h, "the fetch to finish", |app| !app.is_busy());
    // Git sent exactly what was typed (base64 of "rocky:hunter2").
    assert_eq!(seen.lock().unwrap().as_slice(), ["cm9ja3k6aHVudGVyMg=="]);
    let error = h.state().error().unwrap_or_default().to_string();
    assert!(
        error.contains("Could not fetch"),
        "the server refused it: {error}"
    );
    assert_eq!(question(h.state()), None);

    // Cancelling a question makes Git give up, without sending anything.
    h.get_by_label("Fetch").click();
    wait_for(&mut h, "the question again", |app| question(app).is_some());
    h.get_by_label("Cancel").click();
    wait_for(&mut h, "the fetch to stop", |app| !app.is_busy());
    assert_eq!(seen.lock().unwrap().len(), 1, "no second sign-in was sent");
    assert_eq!(question(h.state()), None);
    drop(h);

    // Signed in to that host here: the account answers, nobody is asked.
    accounts::use_memory_store();
    let account = Account {
        kind: Kind::GitHub,
        base: format!("http://127.0.0.1:{port}"),
        login: "rocky".into(),
        client: None,
    };
    accounts::save(
        &account,
        &accounts::Secret {
            token: "gho_signed_in".into(),
            refresh: None,
            expires: None,
        },
    )
    .unwrap();
    let settings = Settings {
        accounts: vec![account],
        ..Settings::default()
    };
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .wgpu()
        .build_eframe(move |cc| {
            let mut app = App::with_settings(&cc.egui_ctx, settings.clone());
            app.answer_git_questions(&cc.egui_ctx, Path::new(env!("CARGO_BIN_EXE_git-raccoon")));
            app
        });
    let ctx = h.ctx.clone();
    h.state_mut().open(&ctx, dir.to_str().unwrap());
    wait_for(&mut h, "the repo", |app| {
        app.repo().is_some() && !app.is_busy()
    });
    h.get_by_label("Fetch").click();
    let sent = seen.clone();
    wait_for(&mut h, "the sign-in", move |app| {
        assert_eq!(question(app), None, "the account answered");
        sent.lock().unwrap().len() == 2
    });
    wait_for(&mut h, "the fetch to finish", |app| !app.is_busy());
    // Its login and token (base64 of "rocky:gho_signed_in").
    assert_eq!(seen.lock().unwrap()[1], "cm9ja3k6Z2hvX3NpZ25lZF9pbg==");
    let _ = std::fs::remove_dir_all(&dir);
}
