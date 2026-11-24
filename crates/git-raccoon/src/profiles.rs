//! Profiles: who you commit as (a name and email), and which signed-in
//! accounts push, pull and show requests. One is active for the whole app.
//!
//! Accounts aren't owned by a profile: they're signed in once (Settings ›
//! Accounts) and any number of profiles can use the same one, at most one
//! per forge in each.

use serde::{Deserialize, Serialize};

use crate::forge::Account;

/// A profile, as saved with the settings.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    /// Which of the profile colours (see `ui::profiles::COLOURS`).
    pub colour: usize,
    /// The name and email to commit as; empty: Git's own.
    pub commit_name: String,
    pub email: String,
    /// The accounts it uses, at most one per forge.
    pub accounts: Vec<AccountRef>,
}

/// Which signed-in account: its forge's address and the name on it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AccountRef {
    pub base: String,
    pub login: String,
}

impl AccountRef {
    pub fn of(a: &Account) -> AccountRef {
        AccountRef {
            base: a.base.clone(),
            login: a.login.clone(),
        }
    }

    pub fn is(&self, a: &Account) -> bool {
        self.base == a.base && self.login == a.login
    }
}

impl Profile {
    pub fn uses(&self, a: &Account) -> bool {
        self.accounts.iter().any(|r| r.is(a))
    }

    /// Uses `a` for its forge, in place of the one it had there.
    pub fn take_account(&mut self, a: &Account) {
        self.accounts.retain(|r| r.base != a.base);
        self.accounts.push(AccountRef::of(a));
    }

    pub fn drop_account(&mut self, a: &Account) {
        self.accounts.retain(|r| !r.is(a));
    }

    /// Its account for `base` (a forge), if it has one.
    pub fn has_forge(&self, base: &str) -> bool {
        self.accounts.iter().any(|r| r.base == base)
    }

    /// The first letter of its name, for its badge.
    pub fn initial(&self) -> String {
        self.name
            .trim()
            .chars()
            .next()
            .map_or_else(|| "?".to_string(), |c| c.to_uppercase().collect())
    }

    /// "Rocky Raccoon · rocky@acme.example", or what's missing.
    pub fn who(&self) -> String {
        let name = if self.commit_name.trim().is_empty() {
            "Git's name"
        } else {
            self.commit_name.trim()
        };
        let email = if self.email.trim().is_empty() {
            "Git's email"
        } else {
            self.email.trim()
        };
        format!("{name} · {email}")
    }

    /// The name and email to commit as, when both are set.
    pub fn identity(&self) -> Option<(String, String)> {
        let (name, email) = (self.commit_name.trim(), self.email.trim());
        (!name.is_empty() && !email.is_empty()).then(|| (name.to_string(), email.to_string()))
    }
}

/// The profile made on first run: Git's own name and email, and every
/// signed-in account (the first for each forge), so nothing changes until
/// a second profile is added.
pub fn first(accounts: &[Account], name: &str, email: &str) -> Profile {
    let mut p = Profile {
        name: "Default".into(),
        colour: 0,
        commit_name: name.to_string(),
        email: email.to_string(),
        accounts: Vec::new(),
    };
    for a in accounts {
        if !p.has_forge(&a.base) {
            p.accounts.push(AccountRef::of(a));
        }
    }
    p
}

/// Git's own `user.name` and `user.email` (global), where set.
pub fn git_identity() -> (String, String) {
    git_raccoon_core::commit::global_identity()
}

/// A warning that the repository belongs to an account the profile in use
/// doesn't have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub text: String,
    /// The profile that has it, to switch to.
    pub switch: Option<usize>,
}

/// When the repository is on an account's own space ("github.com/rocky/…"
/// with rocky signed in) and only another profile has that account: which
/// one. Nothing to say when the profile in use has it, when the owner is
/// no signed-in account (an organisation's, say), or when several
/// profiles have it, one of them in use.
pub fn owner_warning(
    profiles: &[Profile],
    active: usize,
    accounts: &[Account],
    project: &crate::forge::Project,
) -> Option<Warning> {
    let owner = project.path.split('/').next()?;
    let theirs: Vec<&Account> = accounts
        .iter()
        .filter(|a| a.base == project.base && a.login.eq_ignore_ascii_case(owner))
        .collect();
    let account = theirs.first()?;
    let place = format!("{}/{owner}", project.host);
    if profiles.get(active).is_some_and(|p| p.uses(account)) {
        return None;
    }
    match profiles.iter().position(|p| p.uses(account)) {
        Some(i) => Some(Warning {
            text: format!(
                "This repository is on {place}, an account in your {} profile.",
                profiles[i].name
            ),
            switch: Some(i),
        }),
        None => Some(Warning {
            text: format!(
                "This repository is on {place}, an account no profile uses: it won't be used for pushing or its requests."
            ),
            switch: None,
        }),
    }
}

/// A name for a new profile not already taken: "New profile", "New
/// profile 2"…
pub fn free_name(profiles: &[Profile], base: &str) -> String {
    let taken = |n: &str| profiles.iter().any(|p| p.name == n);
    if !taken(base) {
        return base.to_string();
    }
    (2..)
        .map(|i| format!("{base} {i}"))
        .find(|n| !taken(n))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::Kind;

    fn account(base: &str, login: &str) -> Account {
        Account {
            kind: Kind::GitHub,
            base: base.into(),
            login: login.into(),
            client: None,
        }
    }

    #[test]
    fn one_account_per_forge() {
        let (a, b, c) = (
            account("https://github.com", "rocky-acme"),
            account("https://github.com", "rockyraccoon"),
            account("https://gitlab.com", "rocky"),
        );
        let first = first(&[a.clone(), b.clone(), c.clone()], "Rocky", "r@x");
        assert!(first.uses(&a) && !first.uses(&b) && first.uses(&c));
        let mut p = first.clone();
        p.take_account(&b);
        assert!(!p.uses(&a) && p.uses(&b) && p.uses(&c), "swapped in");
        p.drop_account(&c);
        assert_eq!(p.accounts.len(), 1);
        assert_eq!(p.initial(), "D");
        assert_eq!(p.identity(), Some(("Rocky".into(), "r@x".into())));
        assert_eq!(Profile::default().who(), "Git's name · Git's email");
        assert_eq!(Profile::default().identity(), None);
        assert_eq!(free_name(&[first], "Default"), "Default 2");
    }

    #[test]
    fn warns_when_another_profile_has_the_owner() {
        let (work, me) = (
            account("https://github.com", "rocky-acme"),
            account("https://github.com", "rockyraccoon"),
        );
        let accounts = [work.clone(), me.clone()];
        let profiles = [
            Profile {
                name: "Work".into(),
                accounts: vec![AccountRef::of(&work)],
                ..Profile::default()
            },
            Profile {
                name: "Personal".into(),
                accounts: vec![AccountRef::of(&me)],
                ..Profile::default()
            },
        ];
        let on = |path: &str| crate::forge::Project {
            kind: Kind::GitHub,
            base: "https://github.com".into(),
            host: "github.com".into(),
            path: path.into(),
        };
        let w = owner_warning(&profiles, 0, &accounts, &on("RockyRaccoon/notes")).unwrap();
        assert_eq!(
            w.text,
            "This repository is on github.com/RockyRaccoon, an account in your Personal profile."
        );
        assert_eq!(w.switch, Some(1));
        assert_eq!(
            owner_warning(&profiles, 1, &accounts, &on("rockyraccoon/notes")),
            None
        );
        assert_eq!(
            owner_warning(&profiles, 0, &accounts, &on("acme/app")),
            None,
            "an org's"
        );
        assert_eq!(
            owner_warning(&profiles, 0, &accounts, &on("rocky-acme/x")),
            None
        );
        // In no profile at all.
        let unused = [profiles[0].clone()];
        let w = owner_warning(&unused, 0, &accounts, &on("rockyraccoon/notes")).unwrap();
        assert_eq!(w.switch, None);
    }
}
