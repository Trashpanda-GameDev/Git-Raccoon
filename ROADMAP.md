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

- [ ] Multiple repos in tabs
- [ ] Command palette and keyboard shortcuts for every action
- [ ] File history and blame
- [ ] Light and dark themes; settings screen
- [ ] Submodules, worktrees and Git LFS awareness
- [ ] Performance target: the graph stays smooth on a repo with 1M+ commits

## v1.0: Ship

- [ ] CI on GitHub Actions: build and test on Linux, macOS and Windows
- [ ] Installers and in-app updates (for example Velopack, as in the Asset Browser): Windows first, then macOS and Linux
- [ ] Opt-in crash reporting
- [ ] User guide and project website
- [x] App icon
- [ ] Branding: website and installer artwork

## Later (ideas, not commitments)

- GitHub and GitLab integration: pull requests, reviews and issues next to the graph
- Commit message suggestions from the staged diff
- Built-in merge tool for three-way merges
- Workspaces that group related repos
