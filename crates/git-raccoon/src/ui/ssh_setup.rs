//! Set up SSH for GitHub/GitLab: everything from nothing to a working
//! push, in one window, for a signed-in account. Steps already done are
//! ticked and skipped; each says what it changes before doing it, and can
//! be done by hand instead.

use std::path::PathBuf;

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;
use git_raccoon_core::ssh::{self, Agent, Connection, Key, Ssh};

use crate::forge::accounts::Account;
use crate::forge::ssh_keys::{self, AccountKey, KeyError};
use crate::repo_view::Op;
use crate::task::Task;
use crate::theme;
use crate::ui::ssh_settings::account_host;

/// A repository whose remote can be switched to SSH at the end.
#[derive(Clone)]
pub struct ForRepo {
    pub path: String,
    pub remote: String,
    pub url: String,
}

/// What's there now.
struct Status {
    keys: Vec<Key>,
    agent: Agent,
    account_keys: Result<Vec<AccountKey>, KeyError>,
}

/// How a step's change went: a new key, if it made one.
type StepResult = Result<Option<PathBuf>, String>;

/// The setup window's state.
pub struct Setup {
    pub account: Account,
    pub repo: Option<ForRepo>,
    status: Option<Status>,
    loading: Option<Task<Status>>,
    /// The key being set up (its private file).
    key: Option<PathBuf>,
    /// A new key's name and passphrase (twice).
    name: String,
    passphrase: String,
    again: String,
    no_passphrase: bool,
    /// The key's title on the account.
    title: String,
    /// A change running; a new key comes back from it.
    job: Option<(String, Task<StepResult>)>,
    problem: Option<String>,
    /// Steps the person said they'd do by hand.
    by_hand_agent: bool,
    /// The connection test, running or done.
    testing: Option<Task<Connection>>,
    tested: Option<Connection>,
}

impl Setup {
    pub fn new(account: Account, repo: Option<ForRepo>) -> Self {
        let dir = ssh::ssh_dir();
        Setup {
            account,
            repo,
            status: None,
            loading: None,
            key: None,
            name: dir.as_deref().map_or_else(
                || "id_ed25519".into(),
                |d| ssh::free_key_name(d, "id_ed25519"),
            ),
            passphrase: String::new(),
            again: String::new(),
            no_passphrase: false,
            title: ssh_keys::default_title(),
            job: None,
            problem: None,
            by_hand_agent: false,
            testing: None,
            tested: None,
        }
    }

    /// Whether it's running something that may ask a question (a
    /// passphrase, a new host).
    pub fn asking(&self) -> bool {
        self.job.is_some() || self.testing.is_some()
    }

    fn ssh(&self) -> Ssh {
        match &self.repo {
            Some(r) => Ssh::for_git(Some(&r.path)),
            None => Ssh::for_app(),
        }
    }

    fn host(&self) -> String {
        account_host(&self.account)
    }

    fn reload(&mut self, ctx: &egui::Context) {
        let account = self.account.clone();
        let ssh = self.ssh();
        self.loading = Some(Task::spawn(ctx, move || Status {
            keys: ssh::ssh_dir().as_deref().map(ssh::keys).unwrap_or_default(),
            agent: ssh::agent(&ssh),
            account_keys: ssh_keys::keys(&account),
        }));
    }
}

/// What to do once chosen.
pub enum Done {
    /// Run this (switching the repository's remote to SSH).
    Run(Op),
    Close,
}

fn step(
    ui: &mut egui::Ui,
    n: usize,
    done: bool,
    current: bool,
    title: &str,
    about: &str,
    body: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal_top(|ui| {
        let (text, fill, colour) = if done {
            (
                icon::CHECK.to_string(),
                theme::add().gamma_multiply(0.25),
                theme::add(),
            )
        } else if current {
            (n.to_string(), theme::accent_fill(), theme::on_accent())
        } else {
            (n.to_string(), egui::Color32::TRANSPARENT, theme::muted())
        };
        egui::Frame::new()
            .fill(fill)
            .stroke(egui::Stroke::new(
                1.0,
                if current || done {
                    fill
                } else {
                    theme::border()
                },
            ))
            .corner_radius(12)
            .inner_margin(egui::Margin::symmetric(7, 2))
            .show(ui, |ui| {
                ui.label(RichText::new(text).size(12.0).color(colour));
            });
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                let t = RichText::new(title).strong();
                ui.label(if done || current {
                    t
                } else {
                    t.color(theme::muted())
                });
                if !about.is_empty() {
                    ui.label(RichText::new(format!("· {about}")).color(theme::muted()));
                }
            });
            if current {
                body(ui);
            }
        });
    });
    ui.add_space(8.0);
}

fn primary(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).color(theme::on_accent()).strong())
            .fill(theme::accent_fill()),
    )
}

fn name_of(p: &std::path::Path) -> String {
    p.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// Shows the setup. Returns what to do once chosen.
pub fn show(ctx: &egui::Context, s: &mut Setup) -> Option<Done> {
    if s.status.is_none() && s.loading.is_none() {
        s.reload(ctx);
    }
    if let Some(done) = s.loading.as_ref().and_then(Task::poll) {
        // A key to set up: the one chosen, else the newest ed25519, else any.
        if s.key.is_none() {
            s.key = done
                .keys
                .iter()
                .find(|k| k.kind == "ED25519" && k.public.is_some())
                .or_else(|| done.keys.iter().find(|k| k.public.is_some()))
                .map(|k| k.path.clone());
        }
        s.status = Some(done);
        s.loading = None;
    }
    if let Some((_, task)) = &s.job
        && let Some(result) = task.poll()
    {
        s.job = None;
        match result {
            Ok(Some(new_key)) => {
                s.key = Some(new_key);
                s.problem = None;
            }
            Ok(None) => s.problem = None,
            Err(e) => s.problem = Some(e),
        }
        s.reload(ctx);
    }
    if let Some(c) = s.testing.as_ref().and_then(Task::poll) {
        s.tested = Some(c);
        s.testing = None;
    }
    let place = s.account.place();
    let host = s.host();
    let mut done = None;
    let modal = egui::Modal::new(egui::Id::new("ssh-setup")).show(ctx, |ui| {
        ui.set_width(600.0);
        ui.label(RichText::new(format!("Set up SSH for {place}")).strong().size(16.0));
        ui.label(
            RichText::new(format!(
                "Signed in as {}. About a minute; nothing changes until a step says so.",
                s.account.login
            ))
            .color(theme::muted()),
        );
        ui.add_space(10.0);
        let Some(status) = &s.status else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Looking at your SSH setup…").color(theme::muted()));
            });
            return;
        };
        let busy = s.job.is_some() || s.loading.is_some();
        let key = s
            .key
            .as_ref()
            .and_then(|p| status.keys.iter().find(|k| &k.path == p))
            .cloned();
        let fp = key.as_ref().and_then(|k| k.fingerprint.clone()).unwrap_or_default();
        // Which steps are done.
        let have_key = key.is_some();
        let needs_agent = key.as_ref().is_some_and(|k| k.passphrase != Some(false));
        let in_agent = have_key
            && (!needs_agent || s.by_hand_agent || status.agent.keys.iter().any(|a| a.fingerprint == fp));
        let on_account = match &status.account_keys {
            Ok(list) => list.iter().any(|k| k.fingerprint.as_deref() == Some(fp.as_str())),
            Err(_) => false,
        };
        let works = matches!(s.tested, Some(Connection::SignedIn(_)));
        let repo_ssh = s.repo.as_ref().map(|r| ssh::parse_url(&r.url).is_some());
        let current = if !have_key {
            1
        } else if !in_agent {
            2
        } else if !on_account {
            3
        } else if !works {
            4
        } else if repo_ssh == Some(false) {
            5
        } else {
            6
        };
        // The test runs by itself once the steps before it are done.
        if current == 4 && s.testing.is_none() && s.tested.is_none() {
            let ssh = s.ssh();
            let h = host.clone();
            s.testing = Some(Task::spawn(ui.ctx(), move || {
                ssh::test_connection(&ssh, "git", &h, None).unwrap_or_else(|e| Connection::Other(e.to_string()))
            }));
        }

        // 1. A key.
        let about = match &key {
            Some(k) => {
                let pass = if k.passphrase == Some(true) { ", with a passphrase" } else { "" };
                format!("{} in ~/.ssh{pass}", name_of(&k.path))
            }
            None => String::new(),
        };
        let mut create = None;
        step(ui, 1, have_key, current == 1, "A key", &about, |ui| {
            ui.label(RichText::new("Makes an ed25519 key in ~/.ssh (never replacing one).").size(12.0).color(theme::muted()));
            egui::Grid::new("setup-key").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(220.0));
                ui.end_row();
                if !s.no_passphrase {
                    ui.label("Passphrase");
                    let f = ui.add(egui::TextEdit::singleline(&mut s.passphrase).password(true).desired_width(220.0));
                    ui.ctx().accesskit_node_builder(f.id, |n| n.set_label("Key passphrase"));
                    ui.end_row();
                    ui.label("Again");
                    let f = ui.add(egui::TextEdit::singleline(&mut s.again).password(true).desired_width(220.0));
                    ui.ctx().accesskit_node_builder(f.id, |n| n.set_label("Key passphrase again"));
                    ui.end_row();
                }
            });
            let none = ui.checkbox(&mut s.no_passphrase, "No passphrase");
            ui.ctx().accesskit_node_builder(none.id, |n| n.set_label("No passphrase"));
            if s.no_passphrase {
                ui.label(RichText::new("Without one, anyone with a copy of the file can use the key.").size(12.0).color(theme::modified()));
            }
            let ok = !s.name.trim().is_empty()
                && (s.no_passphrase || (s.passphrase.chars().count() >= 5 && s.passphrase == s.again));
            if primary(ui, "Create the key", ok && !busy).clicked() {
                create = Some((s.name.trim().to_string(), (!s.no_passphrase).then(|| s.passphrase.clone())));
            }
        });
        if let Some((name, pass)) = create
            && let Some(dir) = ssh::ssh_dir()
        {
            let comment = ssh::default_comment();
            s.job = Some((
                "Creating the key".into(),
                Task::spawn(ui.ctx(), move || {
                    ssh::create_key(&dir, &name, &comment, pass.as_deref())
                        .map(|k| Some(k.path))
                        .map_err(|e| e.to_string())
                }),
            ));
        }

        // 2. In the agent.
        let about = if !have_key {
            String::new()
        } else if !needs_agent {
            "no passphrase to remember".into()
        } else if in_agent {
            "you won't be asked for the passphrase again this session".into()
        } else {
            String::new()
        };
        let mut add_agent = false;
        step(ui, 2, have_key && in_agent, current == 2, "Remembered by the SSH agent", &about, |ui| {
            if !status.agent.running {
                ui.label(RichText::new(format!("No SSH agent is running ({}). Settings › SSH says how to start one.", status.agent.kind.name())).size(12.0).color(theme::modified()));
            }
            ui.label(RichText::new("Adds the key to the agent: you type its passphrase once.").size(12.0).color(theme::muted()));
            ui.horizontal(|ui| {
                add_agent = primary(ui, "Add it to the agent", status.agent.running && !busy).clicked();
                if ui.button("Skip: I'll type the passphrase when asked").clicked() {
                    s.by_hand_agent = true;
                }
            });
        });
        if add_agent && let Some(k) = &key {
            let (ssh, path) = (s.ssh(), k.path.clone());
            s.job = Some((
                "Adding it to the agent".into(),
                Task::spawn(ui.ctx(), move || {
                    ssh::add_to_agent(&ssh, &path).map(|()| None).map_err(|e| e.to_string())
                }),
            ));
        }

        // 3. On the account.
        let mut add_account = false;
        let mut check_again = false;
        let needs_sign_in = matches!(status.account_keys, Err(KeyError::NeedsPermission));
        step(ui, 3, on_account, current == 3, &format!("Add it to your {place} account"), "", |ui| {
            ui.label(RichText::new(format!("Git Raccoon adds it with your sign-in. It shows under {place}'s SSH keys settings.")).size(12.0).color(theme::muted()));
            if needs_sign_in {
                ui.label(RichText::new(KeyError::NeedsPermission.describe(&s.account)).size(12.0).color(theme::modified()));
            }
            ui.horizontal(|ui| {
                ui.label("Title");
                let f = ui.add(egui::TextEdit::singleline(&mut s.title).desired_width(260.0));
                ui.ctx().accesskit_node_builder(f.id, |n| n.set_label("Key title"));
            });
            ui.horizontal(|ui| {
                add_account = primary(ui, &format!("Add to {place}"), !busy && !needs_sign_in).clicked();
                if let Some(public) = key.as_ref().and_then(|k| k.public.clone())
                    && ui.button("Copy it and add it myself").clicked()
                {
                    ui.ctx().copy_text(public);
                    s.problem = Some(format!("Copied. Add it on {place} (SSH keys settings), then Check again."));
                }
                check_again = ui.button("Check again").clicked();
            });
        });
        if add_account && let Some(public) = key.as_ref().and_then(|k| k.public.clone()) {
            let (account, title) = (s.account.clone(), s.title.clone());
            s.job = Some((
                format!("Adding it to {place}"),
                Task::spawn(ui.ctx(), move || {
                    ssh_keys::add_key(&account, &title, &public)
                        .map(|_| None)
                        .map_err(|e| e.describe(&account))
                }),
            ));
        }
        if check_again {
            s.reload(ui.ctx());
        }

        // 4. It works.
        let about = match &s.tested {
            Some(Connection::SignedIn(_)) => s.tested.as_ref().map(|c| c.describe(&host)).unwrap_or_default(),
            _ => format!("connects to {host} as you"),
        };
        let mut retest = false;
        step(ui, 4, works, current == 4, "Check it works", &about, |ui| {
            match &s.tested {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(format!("Connecting to {host}…")).size(12.0).color(theme::muted()));
                    });
                }
                Some(c) => {
                    ui.label(RichText::new(c.describe(&host)).size(12.0).color(theme::danger()));
                    retest = ui.button("Test again").clicked();
                }
            }
        });
        if retest {
            s.tested = None;
        }

        // 5. This repository over SSH.
        if let Some(r) = s.repo.clone() {
            let to = ssh::ssh_url(&r.url).unwrap_or_default();
            let about = if repo_ssh == Some(true) {
                format!("{} uses SSH", r.remote)
            } else {
                format!("{}: {} {} {to}", r.remote, r.url, icon::ARROW_RIGHT)
            };
            step(ui, 5, repo_ssh == Some(true), current == 5, "Use SSH for this repository", &about, |ui| {
                ui.label(RichText::new(format!("Changes {}'s address (you can undo it).", r.remote)).size(12.0).color(theme::muted()));
                if primary(ui, &format!("Use SSH for {}", r.remote), !to.is_empty()).clicked() {
                    done = Some(Done::Run(Op::SetRemoteUrl { name: r.remote.clone(), url: to.clone() }));
                }
            });
        }

        if let Some((what, _)) = &s.job {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(format!("{what}…")).color(theme::muted()));
            });
        } else if let Some(p) = &s.problem {
            ui.label(RichText::new(p).size(12.0).color(theme::modified()));
        }
        if current == 6 {
            ui.label(RichText::new(format!("{}  SSH is set up for {place}.", icon::CHECK)).strong().color(theme::add()));
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let label = if current == 6 { "Done" } else { "Close" };
            if ui.button(label).clicked() {
                done = Some(Done::Close);
            }
        });
    });
    if modal.should_close() && done.is_none() && s.job.is_none() {
        done = Some(Done::Close);
    }
    done
}

impl Setup {
    /// The repository's remote now has this address (switched to SSH).
    pub fn remote_changed(&mut self, url: String) {
        if let Some(r) = &mut self.repo {
            r.url = url;
        }
    }
}
