//! When a fetch, pull or push fails over SSH: find out why (testing the
//! connection, the keys, the agent and the account), say it plainly, and
//! offer the fix, then try again.

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;
use git_raccoon_core::ssh::{self, Connection, Diagnosis, Key, Ssh};

use crate::forge::accounts::Account;
use crate::forge::ssh_keys::{self, AccountKey, KeyError};
use crate::repo_view::Op;
use crate::task::Task;
use crate::theme;
use crate::ui::ssh_settings::account_host;

/// What was found, with the signed-in account for the host (if any) and
/// its keys.
pub struct Found {
    pub diagnosis: Diagnosis,
    pub account: Option<(Account, Result<Vec<AccountKey>, KeyError>)>,
}

/// An operation that failed over SSH, being looked into.
pub struct SshProblem {
    /// What failed, to run again once fixed.
    pub op: Op,
    /// The remote it went to, and its address.
    pub remote: String,
    pub url: String,
    /// What Git said.
    pub said: String,
    found: Option<Found>,
    looking: Option<Task<Found>>,
    fixing: Option<(String, Task<Result<(), String>>)>,
    failed: Option<String>,
    details: bool,
}

impl SshProblem {
    pub fn new(op: Op, remote: String, url: String, said: String) -> Self {
        SshProblem {
            op,
            remote,
            url,
            said,
            found: None,
            looking: None,
            fixing: None,
            failed: None,
            details: false,
        }
    }

    /// Whether it's running SSH (which may ask a passphrase or about a
    /// host): the app keeps such questions up meanwhile.
    pub fn asking(&self) -> bool {
        self.looking.is_some() || self.fixing.is_some()
    }
}

/// Why, in one word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cause {
    /// It works now (it was a passing problem).
    Works,
    /// No key on this computer.
    NoKey,
    /// A key, but SSH didn't offer it (not in the agent).
    NotInAgent(Key),
    /// Offered, and refused: not on the account (`account` when signed in
    /// to that host here).
    NotOnAccount {
        key: Option<Key>,
        account: Option<Account>,
    },
    HostUnknown,
    HostKeyChanged,
    PortBlocked,
    NameNotFound,
    Other(String),
}

/// One of the checks, in order: passed, failed, or not got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub label: String,
    pub ok: Option<bool>,
}

/// The signed-in account for a host (GitHub's port-443 host is GitHub).
fn account_for<'a>(accounts: &'a [Account], host: &str) -> Option<&'a Account> {
    let host = match host {
        "ssh.github.com" => "github.com",
        "altssh.gitlab.com" => "gitlab.com",
        h => h,
    };
    accounts.iter().find(|a| account_host(a) == host)
}

/// What's wrong, and the checks that show it.
pub fn cause(found: &Found) -> (Cause, Vec<Check>) {
    let d = &found.diagnosis;
    let host = d.url.host.clone();
    let agent_fps: Vec<&str> = d
        .agent
        .keys
        .iter()
        .map(|k| k.fingerprint.as_str())
        .collect();
    let fps: Vec<&str> = d
        .keys
        .iter()
        .filter_map(|k| k.fingerprint.as_deref())
        .chain(agent_fps.iter().copied())
        .collect();
    let have_key = !d.keys.is_empty() || !d.agent.keys.is_empty();
    let in_agent = !d.agent.keys.is_empty();
    let on_account = match &found.account {
        Some((_, Ok(list))) => Some(
            list.iter()
                .any(|k| k.fingerprint.as_deref().is_some_and(|f| fps.contains(&f))),
        ),
        _ => None,
    };
    let reachable = !matches!(
        d.connection,
        Connection::PortBlocked | Connection::NameNotFound
    );
    let trusted = reachable.then_some(!matches!(
        d.connection,
        Connection::HostUnknown | Connection::HostKeyChanged
    ));
    let mut checks = vec![
        Check {
            label: "Key found in ~/.ssh".into(),
            ok: Some(have_key),
        },
        Check {
            label: "Loaded in the SSH agent".into(),
            ok: Some(in_agent),
        },
    ];
    if let Some((a, _)) = &found.account {
        checks.push(Check {
            label: format!("On {} ({})", a.place(), a.login),
            ok: on_account,
        });
    }
    checks.push(Check {
        label: format!("{host} trusted"),
        ok: trusted,
    });
    checks.push(Check {
        label: format!("{host} reachable"),
        ok: Some(reachable),
    });
    let first_key = || d.keys.first().cloned();
    let cause = match &d.connection {
        Connection::PortBlocked => Cause::PortBlocked,
        Connection::NameNotFound => Cause::NameNotFound,
        Connection::HostKeyChanged => Cause::HostKeyChanged,
        Connection::HostUnknown => Cause::HostUnknown,
        Connection::SignedIn(_) => Cause::Works,
        Connection::NoKey if !have_key => Cause::NoKey,
        Connection::NoKey => match first_key() {
            Some(k) => Cause::NotInAgent(k),
            None => Cause::NoKey,
        },
        Connection::KeyRefused(offered) => {
            // The key it offered, if it's one of the files here.
            let key = d
                .keys
                .iter()
                .find(|k| {
                    k.fingerprint
                        .as_deref()
                        .is_some_and(|f| offered.iter().any(|o| o == f))
                })
                .cloned()
                .or_else(first_key);
            Cause::NotOnAccount {
                key,
                account: found.account.as_ref().map(|(a, _)| a.clone()),
            }
        }
        Connection::Other(t) => Cause::Other(t.clone()),
    };
    (cause, checks)
}

/// What the person chose.
pub enum Fix {
    /// Run the operation again.
    Retry,
    /// Run these (changing the remote's address, then the operation).
    Run(Op),
    /// Open Settings › SSH.
    Settings,
    Close,
}

fn file_name(k: &Key) -> String {
    k.path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// Shows the problem; starts looking into it (and runs fixes in the
/// background). Returns what to do once chosen.
pub fn show(
    ctx: &egui::Context,
    p: &mut SshProblem,
    accounts: &[Account],
    repo: &str,
) -> Option<Fix> {
    // Look into it.
    if p.found.is_none() && p.looking.is_none() {
        let Some(url) = ssh::parse_url(&p.url) else {
            return Some(Fix::Close);
        };
        let account = account_for(accounts, &url.host).cloned();
        let path = repo.to_string();
        p.looking = Some(Task::spawn(ctx, move || {
            let diagnosis = ssh::diagnose(&Ssh::for_git(Some(&path)), &url);
            let account = account.map(|a| {
                let keys = ssh_keys::keys(&a);
                (a, keys)
            });
            Found { diagnosis, account }
        }));
    }
    if let Some(found) = p.looking.as_ref().and_then(Task::poll) {
        p.found = Some(found);
        p.looking = None;
    }
    // A fix running in the background: retry once it's done.
    if let Some((_, task)) = &p.fixing
        && let Some(result) = task.poll()
    {
        p.fixing = None;
        match result {
            Ok(()) => return Some(Fix::Retry),
            Err(e) => p.failed = Some(e),
        }
    }
    let verb = p.op.verb();
    let mut chosen = None;
    let modal = egui::Modal::new(egui::Id::new("ssh-problem")).show(ctx, |ui| {
        ui.set_width(520.0);
        let Some(found) = &p.found else {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::WARNING).color(theme::danger()));
                ui.label(RichText::new(format!("Couldn't {verb} over SSH")).strong().size(15.0));
            });
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Finding out why…").color(theme::muted()));
            });
            return;
        };
        let (cause, checks) = cause(found);
        let host = found.diagnosis.url.host.clone();
        let forge = found
            .account
            .as_ref()
            .map_or_else(|| host.clone(), |(a, _)| a.place());
        let (title, body) = words(&cause, verb, &host, &forge, &found.diagnosis);
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon::WARNING).color(theme::danger()).size(16.0));
            ui.label(RichText::new(title).strong().size(15.0));
        });
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(body).color(theme::muted())).wrap());
        ui.add_space(8.0);
        egui::Frame::new()
            .stroke(egui::Stroke::new(1.0, theme::border()))
            .corner_radius(6)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for c in &checks {
                    let (glyph, colour) = match c.ok {
                        Some(true) => (icon::CHECK, theme::add()),
                        Some(false) => (icon::X, theme::danger()),
                        None => (icon::MINUS, theme::muted()),
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(glyph).color(colour));
                        let text = RichText::new(&c.label);
                        ui.label(if c.ok.is_none() { text.color(theme::muted()) } else { text });
                    });
                }
            });
        if let Some(e) = &p.failed {
            ui.add_space(6.0);
            ui.label(RichText::new(e).size(12.0).color(theme::danger()));
        }
        if p.details {
            ui.add_space(6.0);
            ui.label(RichText::new(p.said.trim()).monospace().size(11.0).color(theme::muted()));
        }
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            if let Some((what, _)) = &p.fixing {
                ui.spinner();
                ui.label(RichText::new(format!("{what}…")).color(theme::muted()));
                return;
            }
            if ui.button("Close").clicked() {
                chosen = Some(Fix::Close);
            }
            if ui.button(if p.details { "Hide details" } else { "Details" }).clicked() {
                p.details = !p.details;
            }
            let https = ssh::https_url(&p.url);
            let offer_https = |ui: &mut egui::Ui, chosen: &mut Option<Fix>| {
                if let Some(url) = &https {
                    let label = format!("Use HTTPS for {} instead", p.remote);
                    let b = ui.button(&label).on_hover_text(format!("Changes {}'s address to {url}", p.remote));
                    if b.clicked() {
                        *chosen = Some(Fix::Run(Op::Then(vec![
                            Op::SetRemoteUrl {
                                name: p.remote.clone(),
                                url: url.clone(),
                            },
                            p.op.clone(),
                        ])));
                    }
                }
            };
            match &cause {
                Cause::Works | Cause::NameNotFound | Cause::Other(_) => {
                    if primary(ui, "Try again").clicked() {
                        chosen = Some(Fix::Retry);
                    }
                }
                Cause::NoKey => {
                    offer_https(ui, &mut chosen);
                    if primary(ui, "Set up SSH…").clicked() {
                        chosen = Some(Fix::Settings);
                    }
                }
                Cause::NotInAgent(k) => {
                    offer_https(ui, &mut chosen);
                    let name = file_name(k);
                    if primary(ui, &format!("Add {name} to the agent and retry")).clicked() {
                        let path = k.path.clone();
                        let repo = repo.to_string();
                        p.failed = None;
                        p.fixing = Some((
                            "Adding it to the agent".into(),
                            Task::spawn(ui.ctx(), move || {
                                ssh::add_to_agent(&Ssh::for_git(Some(&repo)), &path).map_err(|e| e.to_string())
                            }),
                        ));
                    }
                }
                Cause::NotOnAccount { key, account } => {
                    offer_https(ui, &mut chosen);
                    match (key.as_ref().and_then(|k| k.public.clone()), account) {
                        (Some(public), Some(a)) => {
                            let label = format!("Add the key to {} and retry", a.place());
                            if primary(ui, &label).clicked() {
                                let a = a.clone();
                                p.failed = None;
                                p.fixing = Some((
                                    format!("Adding the key to {}", a.place()),
                                    Task::spawn(ui.ctx(), move || {
                                        ssh_keys::add_key(&a, &ssh_keys::default_title(), &public)
                                            .map(drop)
                                            .map_err(|e| e.describe(&a))
                                    }),
                                ));
                            }
                        }
                        (Some(public), None) => {
                            if ui.button(format!("{}  Copy public key", icon::COPY)).clicked() {
                                ui.ctx().copy_text(public);
                            }
                            if primary(ui, "Set up SSH…").clicked() {
                                chosen = Some(Fix::Settings);
                            }
                        }
                        _ => {
                            if primary(ui, "Set up SSH…").clicked() {
                                chosen = Some(Fix::Settings);
                            }
                        }
                    }
                }
                Cause::HostUnknown => {
                    offer_https(ui, &mut chosen);
                    let label = format!("Check {host}'s key and retry");
                    if primary(ui, &label).clicked() {
                        // SSH asks to trust it, with the published-key check.
                        let url = found.diagnosis.url.clone();
                        let repo = repo.to_string();
                        p.failed = None;
                        p.fixing = Some((
                            format!("Connecting to {host}"),
                            Task::spawn(ui.ctx(), move || {
                                let user = if url.user.is_empty() { "git" } else { &url.user };
                                match ssh::test_connection(&Ssh::for_git(Some(&repo)), user, &url.host, url.port) {
                                    Ok(Connection::HostUnknown | Connection::HostKeyChanged) => {
                                        Err(format!("{} isn't trusted.", url.host))
                                    }
                                    Ok(_) => Ok(()),
                                    Err(e) => Err(e.to_string()),
                                }
                            }),
                        ));
                    }
                }
                Cause::HostKeyChanged => {
                    offer_https(ui, &mut chosen);
                    if primary(ui, "Check it in SSH settings").clicked() {
                        chosen = Some(Fix::Settings);
                    }
                }
                Cause::PortBlocked => {
                    offer_https(ui, &mut chosen);
                    if let Some(url) = ssh::port_443_url(&p.url) {
                        let b = primary(ui, "Use port 443")
                            .on_hover_text(format!("GitHub also answers SSH on port 443, which firewalls let through: changes {}'s address to {url}", p.remote));
                        if b.clicked() {
                            chosen = Some(Fix::Run(Op::Then(vec![
                                Op::SetRemoteUrl {
                                    name: p.remote.clone(),
                                    url,
                                },
                                p.op.clone(),
                            ])));
                        }
                    }
                }
            }
        });
    });
    if modal.should_close() && chosen.is_none() && p.fixing.is_none() {
        chosen = Some(Fix::Close);
    }
    chosen
}

fn primary(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(theme::on_accent()).strong())
            .fill(theme::accent_fill()),
    )
}

/// The title and the sentence for a cause.
fn words(cause: &Cause, verb: &str, host: &str, forge: &str, d: &Diagnosis) -> (String, String) {
    match cause {
        Cause::Works => (
            format!("SSH to {host} works now"),
            format!(
                "Testing the connection to {host} just worked: it was a passing problem. Try again."
            ),
        ),
        Cause::NoKey => (
            format!("Couldn't {verb}: you have no SSH key for {forge}"),
            format!(
                "Git reaches {host} over SSH, which needs a key on this computer, added to your account there. Set one up, or use HTTPS (signing in with your account) instead."
            ),
        ),
        Cause::NotInAgent(k) => (
            format!("Couldn't {verb}: your SSH key wasn't offered"),
            format!(
                "You have a key ({}), but SSH didn't offer it to {host}: it isn't in the SSH agent.",
                file_name(k)
            ),
        ),
        Cause::NotOnAccount { key, account } => (
            format!("Couldn't {verb}: {forge} didn't accept your SSH key"),
            match (key, account) {
                (Some(k), Some(a)) => format!(
                    "You have a key ({}), but it isn't on your {forge} account {}.",
                    file_name(k),
                    a.login
                ),
                (Some(k), None) => format!(
                    "You have a key ({}), but {host} doesn't know it: add its public key to your account there (or it's on another account).",
                    file_name(k)
                ),
                _ => format!("{host} refused the key SSH offered: it isn't on your account there."),
            },
        ),
        Cause::HostUnknown => (
            format!("Couldn't {verb}: {host} isn't trusted yet"),
            format!(
                "SSH hasn't connected to {host} before, and needs you to check it's really {forge} first."
            ),
        ),
        Cause::HostKeyChanged => (
            format!("Couldn't {verb}: {host}'s key has changed"),
            format!(
                "SSH refuses to connect: someone may be between you and {host}, pretending to be it, or {host} changed its key. Don't trust the new key until it's checked."
            ),
        ),
        Cause::PortBlocked => (
            format!("Couldn't {verb}: {host} can't be reached over SSH"),
            format!(
                "Nothing answered on port {}: a firewall (at work, or on public Wi-Fi) may be blocking SSH.",
                d.url.port.unwrap_or(22)
            ),
        ),
        Cause::NameNotFound => (
            format!("Couldn't {verb}: {host} wasn't found"),
            "Check the remote's address, and that you're online.".into(),
        ),
        Cause::Other(t) => (
            format!("Couldn't {verb}: SSH couldn't connect to {host}"),
            t.clone(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_raccoon_core::ssh::{Agent, AgentKind, SshUrl};

    fn found(
        connection: Connection,
        keys: usize,
        in_agent: bool,
        account: Option<Vec<&str>>,
    ) -> Found {
        let key = |i: usize| Key {
            path: format!("/home/r/.ssh/id_{i}").into(),
            public: Some(format!("ssh-ed25519 AAAA{i}")),
            kind: "ED25519".into(),
            fingerprint: Some(format!("SHA256:k{i}")),
            comment: String::new(),
            passphrase: Some(false),
            in_config: false,
        };
        let a = Account {
            kind: crate::forge::Kind::GitHub,
            base: "https://github.com".into(),
            login: "rocky".into(),
            client: None,
        };
        Found {
            diagnosis: Diagnosis {
                url: SshUrl {
                    user: "git".into(),
                    host: "github.com".into(),
                    port: None,
                    path: "o/r.git".into(),
                },
                connection,
                keys: (0..keys).map(key).collect(),
                agent: Agent {
                    kind: AgentKind::SshAgent,
                    socket: None,
                    running: true,
                    keys: if in_agent {
                        vec![git_raccoon_core::ssh::AgentKey {
                            bits: 256,
                            fingerprint: "SHA256:k0".into(),
                            comment: String::new(),
                            kind: "ED25519".into(),
                        }]
                    } else {
                        Vec::new()
                    },
                    problem: None,
                },
            },
            account: account.map(|fps| {
                (
                    a,
                    Ok(fps
                        .into_iter()
                        .map(|f| AccountKey {
                            id: 1,
                            title: String::new(),
                            key: String::new(),
                            fingerprint: Some(f.into()),
                        })
                        .collect()),
                )
            }),
        }
    }

    #[test]
    fn finds_the_cause_in_order() {
        let (c, checks) = cause(&found(
            Connection::KeyRefused(vec!["SHA256:k0".into()]),
            1,
            true,
            Some(vec![]),
        ));
        assert!(matches!(
            c,
            Cause::NotOnAccount {
                key: Some(_),
                account: Some(_)
            }
        ));
        let oks: Vec<Option<bool>> = checks.iter().map(|c| c.ok).collect();
        assert_eq!(
            oks,
            [Some(true), Some(true), Some(false), Some(true), Some(true)]
        );
        assert_eq!(checks[2].label, "On GitHub (rocky)");

        let (c, _) = cause(&found(Connection::NoKey, 0, false, None));
        assert_eq!(c, Cause::NoKey);
        let (c, _) = cause(&found(Connection::NoKey, 1, false, None));
        assert!(matches!(c, Cause::NotInAgent(_)));
        let (c, checks) = cause(&found(Connection::PortBlocked, 1, true, None));
        assert_eq!(c, Cause::PortBlocked);
        assert_eq!(checks.last().unwrap().ok, Some(false));
        assert_eq!(checks[checks.len() - 2].ok, None, "trust not got to");
        let (c, _) = cause(&found(
            Connection::SignedIn(Some("rocky".into())),
            1,
            true,
            Some(vec!["SHA256:k0"]),
        ));
        assert_eq!(c, Cause::Works);
        let (c, _) = cause(&found(Connection::HostUnknown, 1, true, None));
        assert_eq!(c, Cause::HostUnknown);
    }
}
