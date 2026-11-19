# Git Raccoon Roadmap

Git Raccoon is a fast, friendly desktop Git client written entirely in Rust: egui/eframe on wgpu for the interface and git2 (libgit2) for Git. It takes inspiration from GitKraken's graph-first workflow, with its own look and identity.

Each milestone is a usable release. Tick items off as they land on `main`.

## Principles

- **Graph first.** The commit graph is the home screen. Every action shows its effect on the graph.
- **Safe by default.** Destructive actions (reset, discard, force-push, branch delete) ask first and can be undone where Git allows it.
- **Respect the user's Git.** Read the user's config, hooks, credential helpers and SSH agent. When git2 can't match the `git` CLI's behaviour for an operation, run `git` instead.
- **Fast on big repos.** Load history incrementally, draw only visible rows, and keep slow work off the UI thread.
- **Tested against real repositories.** Git logic is tested on temporary repos, and UI flows run headlessly against a sample repo with egui_kittest.

## v0.1: Explore (read-only)

A pleasant way to browse any repository.

- [x] Project scaffold: Cargo workspace with `git-raccoon-core` (Git logic) and `git-raccoon` (egui app)
- [x] Native Rust interface (moved from Tauri + React)
- [x] Commit graph with lanes, merge/fork curves and lane colours
- [x] Branch, remote-branch, tag and detached-HEAD labels
- [x] Commit details pane: message, author, date, parents, changed files
- [x] Diff viewer: unified and side-by-side, with rename and binary handling
- [x] Diff viewer: syntax highlighting and word-level changes
- [x] Diff viewer: image previews
- [x] Sidebar: local branches, remotes, tags and stashes; click to jump to the commit in the graph
- [x] Open repos with a native folder picker; recent-repos list on the welcome screen
- [x] Refresh automatically when the repo changes on disk
- [x] Draw only the visible rows, and load more history while scrolling
- [x] Search and filter commits by message, author, SHA or file path

## v0.2: Work (local changes)

Everyday local work without leaving the app.

- [x] Working-changes row at the top of the graph, with staged and unstaged files and their diffs
- [x] Stage and unstage files, hunks and individual lines
- [x] Commit, amend, and a commit message editor with a summary length guide (through the user's own `git`, so hooks and signing apply)
- [x] Discard changes (with confirmation; current versions go to the Recycle Bin / Trash)
- [x] Stash: save, apply, pop and drop
- [x] Branches: create, check out, rename and delete
- [x] Tags: create (lightweight and annotated) and delete
- [x] Undo the last action (commits, amends and checkouts from the reflog; branches, tags and stashes from what the app recorded)

## v0.3: Sync (remotes)

Collaborate through remotes.

- [x] Fetch, pull and push, with progress and cancellation
- [x] Authentication through SSH agent, SSH keys and Git credential helpers, with passwords, key passphrases and new host keys asked in the app
- [x] Ahead/behind counts on branches; set upstream on first push
- [x] Add, edit and remove remotes
- [x] Force-push only with lease, behind a confirmation
- [x] Clone and init a repository from the app

## v0.4: Rewrite (merge and history editing)

The operations people open a GUI for.

- [x] Merge and fast-forward, by dragging one branch onto another (in the sidebar for now; dragging the graph's branch labels to come)
- [x] Rebase onto a branch
- [x] Cherry-pick, revert and reset (soft, mixed, hard)
- [x] Conflict view: list conflicted files, pick ours/theirs per hunk, edit the result
- [x] Interactive rebase: reorder, squash, reword and drop commits by drag and drop
- [x] Clear in-progress banners for merges, rebases and cherry-picks, with continue and abort

## v0.5: Power (depth and comfort)

- [x] Multiple repos in tabs
- [x] Command palette and keyboard shortcuts for every action (every action is in the palette; the frequent ones have shortcuts)
- [x] File history and blame
- [x] Light and dark themes; settings screen (plus Raccoon, Classic, Pastel, Forest, Paper and High contrast looks, each dark or light)
- [x] Submodules, worktrees and Git LFS awareness
- [x] Performance target: the graph stays smooth on a repo with 1M+ commits (opens in ~3 s, frames under 10 ms)

## v1.0: Ship

- [x] CI on GitHub Actions: build and test on Linux, macOS and Windows
- [x] Installers and in-app updates with Velopack: Windows
- [x] Installers for macOS (Apple Silicon: an app and a .pkg) and Linux (an AppImage)
- [x] Opt-in crash reporting (a local report, offered on the next start; sent only as a GitHub issue the user reviews)
- [x] User guide ([docs/guide.md](docs/guide.md))
- [x] Project website ([site/](site/), on GitHub Pages)
- [x] App icon
- [x] Branding: installer artwork and a share card ([scripts/make-artwork.py](scripts/make-artwork.py))
- [x] GitHub, GitLab.com and self-managed GitLab: pull/merge requests (reviews, checks, check out, open new ones) and issues next to the graph; sign in with the browser or a token, kept in the keychain

## Designed (ready to build)

### Profiles: who you commit as, and with which accounts

Mockup and clickable prototype: [Git Raccoon profiles](https://claude.ai/artifact/EHXRYoBeVSrxSwNXLSaveU). Build work: the [Profiles milestone](https://github.com/Trashpanda-GameDev/Git-Raccoon/milestone/1).

- [ ] **A profile** has a name, a colour, the name and email to commit as, and the forge accounts to use (at most one per forge, so it's always clear which account pushes).
- [ ] **Accounts aren't owned by a profile.** They're signed in once, under Settings › Accounts, and any number of profiles can use the same one (one GitHub account with a work and a personal email is the common case). Each profile's editor says which other profiles use an account; signing out of one says which profiles lose it.
- [ ] **One profile is active for the whole app**, picked from a coloured chip at the top right of the toolbar. Its menu lists each profile with its name, email and accounts, plus *New profile…* and *Manage profiles…*.
- [ ] **Settings › Profiles** (above Accounts): the list of profiles, and the chosen one's name, colour, commit name and email, and its accounts (ticking a second account for the same forge swaps it in). *Sign in to another account…* adds an account to that profile. Delete, duplicate, *Use this profile*; saved as you type.
- [ ] **The commit box** says who you're committing as (click to switch), and the button says "Commit 3 files as Work".
- [ ] **A warning** when the repository's remote belongs to an account that only another profile has ("…an account in your Personal profile. Switch to Personal"), or that no profile has. When the account is in several profiles, no warning.
- [ ] **Commits use the profile's name and email** for Git Raccoon's own commits, leaving the user's Git config alone.
- [ ] **First run:** one "Default" profile from the current Git name and email, holding every signed-in account, so nothing changes until a second profile is added.
- Later: remember a profile per repository, or rules by remote (`github.com/acme/*` → Work) or folder; commit signing per profile; an SSH key per profile.

## Later (ideas, not commitments)

- Commit message suggestions from the staged diff
- Built-in merge tool for three-way merges
- Workspaces that group related repos
