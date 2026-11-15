//! Answering Git's and SSH's questions (passwords, key passphrases, "trust
//! this host?") in the app instead of on a terminal it doesn't have.
//!
//! Git and SSH ask an "askpass" program: the app's own executable, started
//! with [`ENV`] set. That process ([`run_if_asked`]) passes the question to
//! the running app over a local connection and prints the answer back to
//! Git. The app sees each question as a [`Request`] and answers it.
//!
//! The connection only listens on 127.0.0.1, and each question must carry
//! a random token known only to this app and the Git processes it starts.

use std::hash::{BuildHasher as _, Hasher as _};
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::mpsc::{self, Receiver, Sender};

/// Set (to "port token") in the environment of Git processes; its presence
/// means "you were started to answer a question".
pub const ENV: &str = "GIT_RACCOON_ASKPASS";

/// A question from Git or SSH, waiting for an answer.
pub struct Request {
    /// What Git asks, such as "Password for 'https://me@example.com': ".
    pub prompt: String,
    reply: Sender<Option<String>>,
}

/// What kind of answer a question wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Something to keep hidden: a password, passphrase or PIN.
    Secret,
    /// Yes or no (SSH's "continue connecting?").
    YesNo,
    /// Plain text, such as a username.
    Text,
}

impl Request {
    pub fn kind(&self) -> Kind {
        kind_of(&self.prompt)
    }

    /// Answers it; `None` cancels, and Git gives up.
    pub fn answer(self, answer: Option<String>) {
        let _ = self.reply.send(answer);
    }
}

fn kind_of(prompt: &str) -> Kind {
    let p = prompt.to_lowercase();
    if p.contains("(yes/no") {
        Kind::YesNo
    } else if p.contains("password")
        || p.contains("passphrase")
        || p.contains("pin ")
        || p.contains("token")
    {
        Kind::Secret
    } else {
        Kind::Text
    }
}

/// Listens for questions for as long as it lives.
pub struct Server {
    requests: Receiver<Request>,
}

impl Server {
    /// Starts listening, and makes Git processes started from now on ask
    /// `program` (the app's executable). `wake` is called when a question
    /// arrives.
    pub fn start(
        program: &Path,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> std::io::Result<Server> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        let token = random_token();
        let (tx, rx) = mpsc::channel();
        let wake = std::sync::Arc::new(wake);
        let expected = token.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (tx, wake, expected) = (tx.clone(), wake.clone(), expected.clone());
                std::thread::spawn(move || {
                    let _ = serve(stream, &expected, &tx, &*wake);
                });
            }
        });
        set_env(Some(AskpassEnv {
            program: program.to_path_buf(),
            value: format!("{port} {token}"),
        }));
        Ok(Server { requests: rx })
    }

    /// The next question, if one has arrived.
    pub fn poll(&self) -> Option<Request> {
        self.requests.try_recv().ok()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        set_env(None);
    }
}

fn serve(
    stream: TcpStream,
    token: &str,
    requests: &Sender<Request>,
    wake: &dyn Fn(),
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if line.trim_end() != token {
        return Ok(()); // not one of ours
    }
    let mut prompt = String::new();
    reader.read_to_string(&mut prompt)?;
    let (reply, answer) = mpsc::channel();
    if requests.send(Request { prompt, reply }).is_err() {
        return Ok(());
    }
    wake();
    // If the app drops the question, that counts as cancelling it.
    let reply = match answer.recv().ok().flatten() {
        Some(text) => format!("ok\n{text}"),
        None => "cancel\n".to_string(),
    };
    let mut stream = stream;
    stream.write_all(reply.as_bytes())
}

/// What a Git process needs to ask the app.
#[derive(Clone)]
pub(crate) struct AskpassEnv {
    pub program: PathBuf,
    /// "port token"
    pub value: String,
}

static CURRENT: RwLock<Option<AskpassEnv>> = RwLock::new(None);

fn set_env(env: Option<AskpassEnv>) {
    if let Ok(mut slot) = CURRENT.write() {
        *slot = env;
    }
}

/// Where Git processes should ask, if the app is listening.
pub(crate) fn current() -> Option<AskpassEnv> {
    CURRENT.read().ok().and_then(|e| e.clone())
}

/// A random hex token. `RandomState` is seeded from the OS's randomness.
fn random_token() -> String {
    let mut out = String::new();
    for i in 0..2u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(i);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// Asks the app at `port` (with `token`) and returns its answer; `None` if
/// the question was cancelled.
fn ask(port: u16, token: &str, prompt: &str) -> std::io::Result<Option<String>> {
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))?;
    stream.write_all(format!("{token}\n{prompt}").as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply)?;
    Ok(reply.strip_prefix("ok\n").map(str::to_string))
}

/// Call first thing in `main`: if this process was started by Git to ask a
/// question, asks the app, prints the answer for Git and returns the exit
/// code to end with. Otherwise returns `None`, and the app starts normally.
pub fn run_if_asked() -> Option<i32> {
    let value = std::env::var(ENV).ok()?;
    let (port, token) = value.split_once(' ')?;
    let port: u16 = port.parse().ok()?;
    let prompt = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    Some(match ask(port, token, &prompt) {
        Ok(Some(answer)) => {
            println!("{answer}");
            0
        }
        _ => 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_question_kinds_apart() {
        assert_eq!(
            kind_of("Password for 'https://me@example.com': "),
            Kind::Secret
        );
        assert_eq!(
            kind_of("Enter passphrase for key '/home/me/.ssh/id_ed25519': "),
            Kind::Secret
        );
        assert_eq!(kind_of("Username for 'https://example.com': "), Kind::Text);
        assert_eq!(
            kind_of(
                "The authenticity of host 'example.com' can't be established.\nAre you sure you want to continue connecting (yes/no/[fingerprint])? "
            ),
            Kind::YesNo
        );
    }

    #[test]
    fn answers_and_cancels_questions_and_checks_the_token() {
        let server = Server::start(Path::new("unused"), || {}).unwrap();
        let env = current().unwrap();
        let (port, token) = env.value.split_once(' ').unwrap();
        let (port, token) = (port.parse::<u16>().unwrap(), token.to_string());

        let answering = std::thread::spawn(move || {
            let mut answered = 0;
            while answered < 2 {
                if let Some(r) = server.poll() {
                    let reply = (answered == 0).then(|| format!("secret for {}", r.prompt));
                    r.answer(reply);
                    answered += 1;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            server
        });
        let multi_line = "line one\nline two: ";
        assert_eq!(
            ask(port, &token, multi_line).unwrap().as_deref(),
            Some("secret for line one\nline two: ")
        );
        assert_eq!(ask(port, &token, "again").unwrap(), None, "cancelled");
        // A wrong token gets no answer at all.
        assert_eq!(ask(port, "wrong", "hello").unwrap(), None);
        drop(answering.join().unwrap());
        assert!(current().is_none(), "stopped with the server");
    }
}
