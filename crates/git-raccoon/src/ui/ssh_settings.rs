//! Settings › SSH: your keys (in the agent? on which accounts?), making or
//! choosing one, the agent, and the hosts SSH trusts. Read in the
//! background; every change says what it does first, and touches nothing
//! outside `~/.ssh` (and, on Windows, only the app's own Git unless asked).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;
use git_raccoon_core::ssh::{
    self, Agent, Connection, Key, KnownHost, Ssh, WindowsAdvice, WindowsSsh,
};

use crate::forge::accounts::Account;
use crate::forge::ssh_keys::{self, AccountKey, KeyError};
use crate::task::Task;
use crate::theme;
use crate::ui::widgets;

/// What there is, read in the background.
pub struct Status {
    pub dir: Option<PathBuf>,
    pub keys: Vec<Key>,
    pub agent: Agent,
    /// Each signed-in account's keys (or why they couldn't be read).
    pub accounts: Vec<(Account, Result<Vec<AccountKey>, KeyError>)>,
    pub hosts: Vec<KnownHost>,
    pub adds_keys: bool,
    pub windows: Option<(WindowsSsh, WindowsAdvice)>,
    /// The ssh the app's Git uses instead of Git's own, if any.
    pub app_ssh: Option<PathBuf>,
    /// The accounts it was read for (read again when they change).
    pub logins: Vec<String>,
}

fn load(accounts: Vec<Account>) -> Status {
    let dir = ssh::ssh_dir();
    let app_ssh = ssh::used_by_app();
    let ssh = Ssh::for_app();
    let agent = if app_ssh.is_some() {
        ssh::agent_at(&ssh, Some(r"\\.\pipe\openssh-ssh-agent".into()))
    } else {
        ssh::agent(&ssh)
    };
    let windows = ssh::windows_ssh().map(|w| {
        let advice = ssh::windows_advice(&Ssh::for_git(None), &w);
        (w, advice)
    });
    Status {
        logins: accounts.iter().map(login_of).collect(),
        keys: dir.as_deref().map(ssh::keys).unwrap_or_default(),
        hosts: dir.as_deref().map(ssh::known_hosts).unwrap_or_default(),
        adds_keys: dir.as_deref().is_some_and(ssh::adds_keys_to_agent),
        accounts: accounts
            .into_iter()
            .map(|a| {
                let keys = ssh_keys::keys(&a);
                (a, keys)
            })
            .collect(),
        agent,
        windows,
        app_ssh,
        dir,
    }
}

fn login_of(a: &Account) -> String {
    format!("{}@{}", a.login, a.base)
}

/// A key being made.
struct Create {
    name: String,
    comment: String,
    passphrase: String,
    again: String,
    none: bool,
    /// Also a `~/.ssh/config` entry for this host.
    host: Option<String>,
}

/// The page's own state, kept between frames.
#[derive(Default)]
struct Page {
    status: Option<Status>,
    loading: Option<Task<Status>>,
    /// A change being made: what, and how it went.
    job: Option<(String, Task<Result<String, String>>)>,
    note: Option<Result<String, String>>,
    /// A GitHub sign-in can't manage keys: offer to sign in again.
    sign_in_again: Option<String>,
    create: Option<Create>,
    tests: Option<Task<Vec<(String, Connection)>>>,
    tested: Vec<(String, Connection)>,
    /// A key chosen from elsewhere, and the host to use it for.
    picked: Option<(PathBuf, Option<String>)>,
}

fn shared(ctx: &egui::Context) -> Arc<Mutex<Page>> {
    let id = egui::Id::new("ssh-settings");
    if let Some(p) = ctx.data(|d| d.get_temp::<Arc<Mutex<Page>>>(id)) {
        return p;
    }
    let p = Arc::new(Mutex::new(Page::default()));
    ctx.data_mut(|d| d.insert_temp(id, Arc::clone(&p)));
    p
}

/// A key file chosen with *Use an existing key…*.
pub fn picked(ctx: &egui::Context, path: PathBuf) {
    if let Ok(mut page) = shared(ctx).lock() {
        page.picked = Some((path, None));
    }
}

/// Keeps the page's background work moving; starts reading if needed.
fn tick(ui: &egui::Ui, accounts: &[Account], page: &mut Page) {
    if let Some(done) = page.loading.as_ref().and_then(Task::poll) {
        page.status = Some(done);
        page.loading = None;
    }
    // Signed in or out since: read the accounts' keys again.
    let logins: Vec<String> = accounts.iter().map(login_of).collect();
    if page.loading.is_none() && page.status.as_ref().is_some_and(|s| s.logins != logins) {
        page.status = None;
        page.sign_in_again = None;
    }
    if page.status.is_none() && page.loading.is_none() {
        let accounts = accounts.to_vec();
        page.loading = Some(Task::spawn(ui.ctx(), move || load(accounts)));
    }
    if let Some(result) = page.job.as_ref().and_then(|(_, t)| t.poll()) {
        page.job = None;
        page.note = Some(result);
        // Whatever changed, read it again.
        let accounts = accounts.to_vec();
        page.loading = Some(Task::spawn(ui.ctx(), move || load(accounts)));
    }
    if let Some(done) = page.tests.as_ref().and_then(Task::poll) {
        page.tested = done;
        page.tests = None;
    }
}

fn start(
    ui: &egui::Ui,
    page: &mut Page,
    what: &str,
    work: impl FnOnce() -> Result<String, String> + Send + 'static,
) {
    page.note = None;
    page.job = Some((what.to_string(), Task::spawn(ui.ctx(), work)));
}

/// "SHA256:Xh3…q9", short enough to read.
fn short_fingerprint(fp: &str) -> String {
    let body = fp.strip_prefix("SHA256:").unwrap_or(fp);
    if body.chars().count() <= 10 {
        return fp.to_string();
    }
    let head: String = body.chars().take(5).collect();
    let tail: String = body
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("SHA256:{head}…{tail}")
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// The host an account's Git goes to: "github.com", "gitlab.example.com".
pub fn account_host(a: &Account) -> String {
    let rest = a.base.split("://").nth(1).unwrap_or(&a.base);
    rest.split(['/', ':']).next().unwrap_or(rest).to_string()
}

/// Shows a file in the system's file manager.
fn show_in_folder(path: &Path) {
    let _ = if cfg!(windows) {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
    } else {
        std::process::Command::new("xdg-open")
            .arg(path.parent().unwrap_or(path))
            .spawn()
    };
}

fn small(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).size(12.0).color(theme::muted())
}

fn badge(ui: &mut egui::Ui, text: &str, colour: egui::Color32) {
    egui::Frame::new()
        .fill(colour.gamma_multiply(0.15))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(colour));
        });
}

fn named(ui: &mut egui::Ui, response: &egui::Response, name: &str) {
    let name = name.to_string();
    ui.ctx()
        .accesskit_node_builder(response.id, |n| n.set_label(name.as_str()));
}

/// While reading: say so. Returns whether there's something to show.
fn loading(ui: &mut egui::Ui, page: &Page) -> bool {
    if page.status.is_some() {
        return true;
    }
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(small("Looking at your SSH setup…"));
    });
    false
}

/// The outcome of the last change, or the one running.
fn note(ui: &mut egui::Ui, page: &mut Page) -> bool {
    let mut sign_in = false;
    if let Some((what, _)) = &page.job {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(small(format!("{what}…")));
        });
    } else if let Some(n) = &page.note {
        let (text, colour) = match n {
            Ok(t) => (format!("{}  {t}", icon::CHECK), theme::add()),
            Err(t) => (format!("{}  {t}", icon::WARNING), theme::danger()),
        };
        ui.add(egui::Label::new(RichText::new(text).size(12.0).color(colour)).wrap());
    }
    if let Some(place) = page.sign_in_again.clone() {
        ui.horizontal(|ui| {
            let b = ui.button(format!("Sign in to {place} again"));
            named(ui, &b, &format!("Sign in to {place} again"));
            sign_in = b.clicked();
        });
    }
    sign_in
}

/// Your keys: each with whether it's in the agent and on which accounts,
/// and the ways to make or choose one. Returns `true` when an action is
/// asked of the app: `Some(true)` choose a key file, `Some(false)` sign in.
pub fn keys_row(ui: &mut egui::Ui, accounts: &[Account]) -> Option<bool> {
    let page = shared(ui.ctx());
    let Ok(mut page) = page.lock() else {
        return None;
    };
    tick(ui, accounts, &mut page);
    let mut wants = None;
    if !loading(ui, &page) {
        return None;
    }
    let busy = page.job.is_some();
    let (keys, in_agent, account_keys, dir) = {
        let s = page.status.as_ref()?;
        (
            s.keys.clone(),
            s.agent
                .keys
                .iter()
                .map(|k| k.fingerprint.clone())
                .collect::<Vec<_>>(),
            s.accounts
                .iter()
                .map(|(a, k)| (a.clone(), k.clone()))
                .collect::<Vec<_>>(),
            s.dir.clone(),
        )
    };
    if keys.is_empty() {
        ui.label(small(
            "No keys in ~/.ssh yet. Create one, or use one you have elsewhere.",
        ));
    }
    for key in &keys {
        let name = file_name(&key.path);
        let fp = key.fingerprint.clone().unwrap_or_default();
        let loaded = !fp.is_empty() && in_agent.contains(&fp);
        egui::Frame::new()
            .stroke(egui::Stroke::new(1.0, theme::border()))
            .corner_radius(6)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&name).monospace().strong());
                    if loaded {
                        badge(ui, "in the agent", theme::add());
                    } else {
                        badge(ui, "not in the agent", theme::modified());
                    }
                    let mut about = vec![key.kind.to_lowercase()];
                    if !fp.is_empty() {
                        about.push(short_fingerprint(&fp));
                    }
                    if !key.comment.is_empty() {
                        about.push(key.comment.clone());
                    }
                    if key.passphrase == Some(true) {
                        about.push("passphrase".into());
                    }
                    ui.label(small(about.join(" · "))).on_hover_text(&fp);
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        if let Some(public) = &key.public {
                            let b = ui.button(format!("{}  Copy public key", icon::COPY));
                            named(ui, &b, &format!("Copy the public key of {name}"));
                            if b.clicked() {
                                ui.ctx().copy_text(public.clone());
                                page.note = Some(Ok(format!(
                                    "Copied {name}'s public key: paste it where a host asks for an SSH key."
                                )));
                            }
                        }
                        if !loaded {
                            let b = ui.button("Add to the agent");
                            named(ui, &b, &format!("Add {name} to the agent"));
                            if b.clicked() {
                                let path = key.path.clone();
                                let name = name.clone();
                                start(ui, &mut page, &format!("Adding {name} to the agent"), move || {
                                    ssh::add_to_agent(&Ssh::for_app(), &path)
                                        .map(|()| format!("{name} is in the agent now."))
                                        .map_err(|e| e.to_string())
                                });
                            }
                        }
                        let b = ui.button(format!("{}  Show in folder", icon::FOLDER_OPEN));
                        named(ui, &b, &format!("Show {name} in its folder"));
                        if b.clicked() {
                            show_in_folder(&key.path);
                        }
                    });
                });
                // On which accounts.
                ui.horizontal_wrapped(|ui| {
                    for (account, listed) in &account_keys {
                        let place = format!("{} ({})", account.place(), account.login);
                        match listed {
                            Ok(list) if list.iter().any(|k| k.fingerprint.as_deref() == Some(fp.as_str())) => {
                                ui.label(
                                    RichText::new(format!("{}  {place}", icon::CHECK))
                                        .size(12.0)
                                        .color(theme::add()),
                                );
                            }
                            Ok(_) => {
                                ui.label(
                                    RichText::new(format!("{}  Not on {place}", icon::DIAMOND))
                                        .size(12.0)
                                        .color(theme::modified()),
                                );
                                let Some(public) = key.public.clone() else { continue };
                                let b = ui.add_enabled(!busy, egui::Button::new(format!("Add to {}", account.place())).small());
                                named(ui, &b, &format!("Add {name} to {place}"));
                                if b.clicked() {
                                    let account = account.clone();
                                    let (name, place) = (name.clone(), place.clone());
                                    start(ui, &mut page, &format!("Adding {name} to {place}"), move || {
                                        ssh_keys::add_key(&account, &ssh_keys::default_title(), &public)
                                            .map(|_| format!("{name} is on {place} now."))
                                            .map_err(|e| e.describe(&account))
                                    });
                                }
                            }
                            Err(KeyError::NeedsPermission) => {
                                ui.label(small(format!("{place}: sign in again to see its keys")));
                                page.sign_in_again = Some(account.place());
                            }
                            Err(e) => {
                                ui.label(small(format!("{place}: {}", e.describe(account))));
                            }
                        }
                    }
                });
                if key.in_config {
                    ui.label(small("Named in ~/.ssh/config"));
                }
            });
        ui.add_space(4.0);
    }
    // Making one, or choosing one from elsewhere.
    if page.create.is_none() && page.picked.is_none() {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!busy, |ui| {
                if widgets::primary_button(ui, &format!("{}  Create a key…", icon::PLUS)).clicked()
                {
                    let name = dir.as_deref().map_or_else(
                        || "id_ed25519".into(),
                        |d| ssh::free_key_name(d, "id_ed25519"),
                    );
                    page.create = Some(Create {
                        name,
                        comment: ssh::default_comment(),
                        passphrase: String::new(),
                        again: String::new(),
                        none: false,
                        host: None,
                    });
                }
                if ui.button("Use an existing key…").clicked() {
                    wants = Some(true);
                }
            });
        });
    }
    let hosts: Vec<String> = accounts.iter().map(account_host).collect();
    create_form(ui, &mut page, dir.as_deref(), &hosts);
    picked_form(ui, &mut page, dir.as_deref(), &hosts);
    if note(ui, &mut page) {
        wants = Some(false);
    }
    wants
}

/// The form for a new key.
fn create_form(ui: &mut egui::Ui, page: &mut Page, dir: Option<&Path>, hosts: &[String]) {
    let Some(dir) = dir.map(Path::to_path_buf) else {
        return;
    };
    let Some(c) = &mut page.create else { return };
    let mut close = false;
    let mut go = None;
    egui::Frame::new()
        .fill(theme::bg())
        .stroke(egui::Stroke::new(1.0, theme::border()))
        .corner_radius(6)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("A new key (ed25519)").strong());
            egui::Grid::new("new-key").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Name");
                let f = ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(220.0));
                named(ui, &f, "Key name");
                ui.end_row();
                ui.label("Comment");
                let f = ui.add(egui::TextEdit::singleline(&mut c.comment).desired_width(220.0));
                named(ui, &f, "Key comment");
                ui.end_row();
                if !c.none {
                    ui.label("Passphrase");
                    let f = ui.add(egui::TextEdit::singleline(&mut c.passphrase).password(true).desired_width(220.0));
                    named(ui, &f, "Key passphrase");
                    ui.end_row();
                    ui.label("Again");
                    let f = ui.add(egui::TextEdit::singleline(&mut c.again).password(true).desired_width(220.0));
                    named(ui, &f, "Key passphrase again");
                    ui.end_row();
                }
            });
            let none = ui.checkbox(&mut c.none, "No passphrase");
            named(ui, &none, "No passphrase");
            if c.none {
                ui.label(
                    RichText::new("Without one, anyone who gets a copy of the key file can use it. The agent can remember a passphrase so you type it once.")
                        .size(12.0)
                        .color(theme::modified()),
                );
            }
            if !hosts.is_empty() {
                ui.horizontal(|ui| {
                    ui.label("Use it for");
                    egui::ComboBox::from_id_salt("new-key-host")
                        .selected_text(c.host.clone().unwrap_or_else(|| "any host (no config entry)".into()))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut c.host, None, "any host (no config entry)");
                            for h in hosts {
                                ui.selectable_value(&mut c.host, Some(h.clone()), h);
                            }
                        });
                });
                if let Some(h) = &c.host {
                    ui.label(small("Adds this to ~/.ssh/config:"));
                    ui.label(RichText::new(ssh::config_entry(h, &dir.join(c.name.trim()), &dir)).monospace().size(12.0));
                }
            }
            let problem = if c.name.trim().is_empty() || c.name.contains(['/', '\\']) {
                Some("Give it a plain file name.")
            } else if !c.none && c.passphrase.chars().count() < 5 {
                Some("A passphrase of five characters or more (or none).")
            } else if !c.none && c.passphrase != c.again {
                Some("The passphrases differ.")
            } else {
                None
            };
            if let Some(p) = problem {
                ui.label(small(p));
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(problem.is_none(), egui::Button::new("Create the key")).clicked() {
                    go = Some((
                        c.name.trim().to_string(),
                        c.comment.trim().to_string(),
                        (!c.none).then(|| c.passphrase.clone()),
                        c.host.clone(),
                    ));
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if let Some((name, comment, passphrase, host)) = go {
        page.create = None;
        start(ui, page, "Creating the key", move || {
            let key = ssh::create_key(&dir, &name, &comment, passphrase.as_deref())
                .map_err(|e| e.to_string())?;
            let made = file_name(&key.path);
            if let Some(h) = host {
                ssh::add_config_entry(&dir, &ssh::config_entry(&h, &key.path, &dir))
                    .map_err(|e| e.to_string())?;
                return Ok(format!(
                    "Created {made}, used for {h}. Add it to your account next."
                ));
            }
            Ok(format!("Created {made}. Add it to your account next."))
        });
    } else if close {
        page.create = None;
    }
}

/// A key chosen from elsewhere: add it to the agent, or use it for a host.
fn picked_form(ui: &mut egui::Ui, page: &mut Page, dir: Option<&Path>, hosts: &[String]) {
    let Some(dir) = dir.map(Path::to_path_buf) else {
        return;
    };
    let Some((path, host)) = &mut page.picked else {
        return;
    };
    let key = ssh::read_key(path);
    let path = path.clone();
    let mut action = None;
    egui::Frame::new()
        .fill(theme::bg())
        .stroke(egui::Stroke::new(1.0, theme::border()))
        .corner_radius(6)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(path.display().to_string()).monospace());
            if key.kind.is_empty() {
                ui.label(
                    RichText::new("This doesn't look like a private key.").color(theme::danger()),
                );
            } else {
                ui.label(small(format!(
                    "{} · {}",
                    key.kind.to_lowercase(),
                    key.fingerprint.clone().unwrap_or_default()
                )));
            }
            ui.horizontal(|ui| {
                if !key.kind.is_empty() && ui.button("Add to the agent").clicked() {
                    action = Some(None);
                }
                if !key.kind.is_empty() && !hosts.is_empty() {
                    egui::ComboBox::from_id_salt("picked-host")
                        .selected_text(host.clone().unwrap_or_else(|| "Use it for…".into()))
                        .show_ui(ui, |ui| {
                            for h in hosts {
                                ui.selectable_value(host, Some(h.clone()), h);
                            }
                        });
                }
                if ui.button("Done").clicked() {
                    action = Some(Some(None));
                }
            });
            if let Some(h) = host.clone() {
                ui.label(small("Adds this to ~/.ssh/config:"));
                ui.label(
                    RichText::new(ssh::config_entry(&h, &path, &dir))
                        .monospace()
                        .size(12.0),
                );
                if ui.button("Add to ~/.ssh/config").clicked() {
                    action = Some(Some(Some(h)));
                }
            }
        });
    match action {
        Some(None) => {
            let p = path.clone();
            start(ui, page, "Adding it to the agent", move || {
                ssh::add_to_agent(&Ssh::for_app(), &p)
                    .map(|()| format!("{} is in the agent now.", file_name(&p)))
                    .map_err(|e| e.to_string())
            });
        }
        Some(Some(Some(h))) => {
            page.picked = None;
            start(ui, page, "Adding it to ~/.ssh/config", move || {
                ssh::add_config_entry(&dir, &ssh::config_entry(&h, &path, &dir))
                    .map(|()| format!("SSH uses {} for {h} now.", file_name(&path)))
                    .map_err(|e| e.to_string())
            });
        }
        Some(Some(None)) => page.picked = None,
        None => {}
    }
}

/// The agent: which, whether it runs, its keys, and adding keys to it
/// when they're first used. On Windows, which ssh the app's Git uses.
pub fn agent_row(ui: &mut egui::Ui, accounts: &[Account]) {
    let page = shared(ui.ctx());
    let Ok(mut page) = page.lock() else { return };
    tick(ui, accounts, &mut page);
    if !loading(ui, &page) {
        return;
    }
    let Some(s) = &page.status else { return };
    let (agent, adds, windows, app_ssh, dir) = (
        s.agent.clone(),
        s.adds_keys,
        s.windows.clone(),
        s.app_ssh.clone(),
        s.dir.clone(),
    );
    let busy = page.job.is_some();
    ui.horizontal(|ui| {
        let (dot, colour) = if agent.running {
            (icon::CIRCLE, theme::add())
        } else {
            (icon::CIRCLE, theme::muted())
        };
        ui.label(RichText::new(dot).color(colour));
        let kind = agent.kind.name();
        let text = if agent.running {
            let n = agent.keys.len();
            let s = if n == 1 { "" } else { "s" };
            format!("Running ({kind}) · {n} key{s} loaded")
        } else {
            format!("Not running ({kind})")
        };
        ui.label(text);
    });
    if let Some(p) = &agent.problem {
        ui.label(small(p.clone()));
    }
    let mut on = adds;
    let toggle = ui.add_enabled(
        !busy && dir.is_some(),
        egui::Checkbox::new(&mut on, "Add keys to the agent when they're first used"),
    );
    named(ui, &toggle, "Add keys to the agent when they're first used");
    ui.label(small(
        "So you type a key's passphrase once, not every time. Turning it on adds AddKeysToAgent to ~/.ssh/config; turning it off removes only that.",
    ));
    if toggle.changed()
        && let Some(dir) = dir
    {
        start(ui, &mut page, "Changing ~/.ssh/config", move || {
            ssh::set_adds_keys_to_agent(&dir, on)
                .map(|()| {
                    if on {
                        "Keys go into the agent when first used.".into()
                    } else {
                        "Keys aren't added to the agent by themselves now.".into()
                    }
                })
                .map_err(|e| e.to_string())
        });
    }
    // Windows: which ssh, and the agent service.
    if let Some(p) = app_ssh {
        ui.add_space(4.0);
        ui.label(small(format!(
            "Your keys are in Windows' agent, which Git's own ssh can't see: Git Raccoon's Git uses Windows' ssh ({}). Git elsewhere still uses its own.",
            p.display()
        )));
        let b = ui.add_enabled(!busy, egui::Button::new("Use it for Git everywhere too"));
        let b = b.on_hover_text(
            "Sets core.sshCommand in your Git config, so Git in a terminal uses it too",
        );
        if b.clicked() {
            start(ui, &mut page, "Setting core.sshCommand", move || {
                ssh::use_ssh_for_git_everywhere(&p)
                    .map(|()| "Git everywhere uses Windows' ssh now (core.sshCommand).".into())
                    .map_err(|e| e.to_string())
            });
        }
    }
    if let Some((_, WindowsAdvice::TurnOnService)) = &windows {
        ui.add_space(4.0);
        ui.label(
            RichText::new("Windows' SSH agent service is turned off, and no agent has your keys. Turning it on takes an administrator: in PowerShell, run as administrator:")
                .size(12.0)
                .color(theme::modified()),
        );
        ui.horizontal(|ui| {
            ui.label(RichText::new(ssh::TURN_ON_SERVICE).monospace().size(12.0));
            if widgets::icon_button(ui, icon::COPY, "Copy").clicked() {
                ui.ctx().copy_text(ssh::TURN_ON_SERVICE.into());
            }
        });
    }
    let r = ui.add(
        egui::Button::new(small(format!("{}  Look again", icon::ARROWS_CLOCKWISE))).frame(false),
    );
    named(ui, &r, "Look at SSH again");
    if r.clicked() {
        page.status = None;
        page.loading = None;
    }
}

/// The hosts SSH trusts, and testing the accounts' hosts.
pub fn hosts_row(ui: &mut egui::Ui, accounts: &[Account]) {
    let page = shared(ui.ctx());
    let Ok(mut page) = page.lock() else { return };
    tick(ui, accounts, &mut page);
    if !loading(ui, &page) {
        return;
    }
    let Some(s) = &page.status else { return };
    let named_hosts: Vec<String> = s
        .hosts
        .iter()
        .filter_map(|h| h.names.first().cloned())
        .collect();
    let hashed = s.hosts.iter().filter(|h| h.names.is_empty()).count();
    if named_hosts.is_empty() && hashed == 0 {
        ui.label(small(
            "None yet: SSH asks the first time it connects to a host.",
        ));
    } else {
        ui.horizontal_wrapped(|ui| {
            for h in &named_hosts {
                ui.label(RichText::new(h).monospace().size(12.0));
            }
            if hashed > 0 {
                let s = if hashed == 1 { "" } else { "s" };
                ui.label(small(format!(
                    "and {hashed} hashed host{s} (their names aren't kept)"
                )));
            }
        });
    }
    let targets: Vec<String> = {
        let mut t: Vec<String> = accounts.iter().map(account_host).collect();
        t.dedup();
        t
    };
    ui.horizontal(|ui| {
        let can = !targets.is_empty() && page.tests.is_none();
        let b = ui
            .add_enabled(can, egui::Button::new("Test connections"))
            .on_disabled_hover_text(
                "Sign in to GitHub or GitLab (Settings › Accounts) to test them",
            );
        if b.clicked() {
            let t = targets.clone();
            page.tested.clear();
            page.tests = Some(Task::spawn(ui.ctx(), move || {
                let ssh = Ssh::for_app();
                t.into_iter()
                    .map(|h| {
                        let c = ssh::test_connection(&ssh, "git", &h, None)
                            .unwrap_or_else(|e| Connection::Other(e.to_string()));
                        (h, c)
                    })
                    .collect()
            }));
        }
        if page.tests.is_some() {
            ui.spinner();
        }
    });
    for (host, c) in &page.tested {
        let ok = matches!(c, Connection::SignedIn(_));
        ui.label(
            RichText::new(format!(
                "{}  {}",
                if ok { icon::CHECK } else { icon::WARNING },
                c.describe(host)
            ))
            .size(12.0)
            .color(if ok { theme::add() } else { theme::danger() }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_fingerprints_and_finds_hosts() {
        assert_eq!(
            short_fingerprint("SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU"),
            "SHA256:+DiY3…COqU"
        );
        let a = |base: &str| Account {
            kind: crate::forge::Kind::GitLab,
            base: base.into(),
            login: "r".into(),
            client: None,
        };
        assert_eq!(
            account_host(&a("https://gitlab.example.com:8443/")),
            "gitlab.example.com"
        );
        assert_eq!(account_host(&a("https://github.com")), "github.com");
    }
}
