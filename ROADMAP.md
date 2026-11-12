# Git Raccoon Roadmap

Git Raccoon is a fast, friendly desktop Git client built on Tauri 2 (Rust + git2) and React. It takes inspiration from GitKraken's graph-first workflow, with its own look and identity.

Each milestone is a usable release. Tick items off as they land on `main`.

## Principles

- **Graph first.** The commit graph is the home screen. Every action shows its effect on the graph.
- **Safe by default.** Destructive actions (reset, discard, force-push, branch delete) ask first and can be undone where Git allows it.
- **Respect the user's Git.** Read the user's config, hooks, credential helpers and SSH agent. When git2 can't match the `git` CLI's behaviour for an operation, run `git` instead.
- **Fast on big repos.** Load history incrementally, draw only visible rows, and keep slow work off the UI thread.
- **Tested against real repositories.** Rust logic is tested on temporary Git repos; UI flows get end-to-end tests.

## v0.1: Explore (read-only)

A pleasant way to browse any repository.

- [x] Project scaffold: Tauri 2, React + TypeScript, git2
- [x] Commit graph with lanes, merge/fork curves and lane colours
- [x] Branch, remote-branch, tag and detached-HEAD labels
- [ ] Commit details pane: message, author, date, parents, changed files
- [ ] Diff viewer: unified and side-by-side, syntax highlighting, image previews
- [ ] Sidebar: local branches, remotes, tags and stashes; click to jump to the commit in the graph
- [ ] Open repos with a native folder picker; recent-repos list on the welcome screen
- [ ] Refresh automatically when the repo changes on disk
- [ ] Draw only the visible rows, and load more history while scrolling
- [ ] Search and filter commits by message, author, SHA or file path

## v0.2: Work (local changes)

Everyday local work without leaving the app.

- [ ] Working-changes row at the top of the graph
- [ ] Stage and unstage files, hunks and individual lines
- [ ] Commit, amend, and a commit message editor with a summary length guide
- [ ] Discard changes (with confirmation)
- [ ] Stash: save, apply, pop and drop
- [ ] Branches: create, check out, rename and delete
- [ ] Tags: create (lightweight and annotated) and delete
- [ ] Undo the last action, using the reflog

## v0.3: Sync (remotes)

Collaborate through remotes.

- [ ] Fetch, pull and push, with progress and cancellation
- [ ] Authentication through SSH agent, SSH keys and Git credential helpers
- [ ] Ahead/behind counts on branches; set upstream on first push
- [ ] Add, edit and remove remotes
- [ ] Force-push only with lease, behind a confirmation
- [ ] Clone and init a repository from the app

## v0.4: Rewrite (merge and history editing)

The operations people open a GUI for.

- [ ] Merge and fast-forward, by dragging one branch onto another in the graph
- [ ] Rebase onto a branch
- [ ] Cherry-pick, revert and reset (soft, mixed, hard)
- [ ] Conflict view: list conflicted files, pick ours/theirs per hunk, edit the result
- [ ] Interactive rebase: reorder, squash, reword and drop commits by drag and drop
- [ ] Clear in-progress banners for merges, rebases and cherry-picks, with continue and abort

## v0.5: Power (depth and comfort)

- [ ] Multiple repos in tabs
- [ ] Command palette and keyboard shortcuts for every action
- [ ] File history and blame
- [ ] Light and dark themes; settings screen
- [ ] Submodules, worktrees and Git LFS awareness
- [ ] Performance target: the graph stays smooth on a repo with 1M+ commits

## v1.0: Ship

- [ ] CI on GitHub Actions: build and test on Linux, macOS and Windows
- [ ] Installers: AppImage and .deb, signed macOS .dmg, Windows .msi
- [ ] Auto-update
- [ ] Opt-in crash reporting
- [ ] User guide and project website
- [ ] App icon and branding

## Later (ideas, not commitments)

- GitHub and GitLab integration: pull requests, reviews and issues next to the graph
- Commit message suggestions from the staged diff
- Built-in merge tool for three-way merges
- Workspaces that group related repos
