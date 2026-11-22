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

### Merge requests without leaving the app

Clickable prototype: [Git Raccoon merge requests](https://claude.ai/artifact/HZ9aRqi9yffGtvRmptrYW8). Build work: the [Merge requests milestone](https://github.com/Trashpanda-GameDev/Git-Raccoon/milestone/2).

Built in 0.5.18, with one set of words for GitHub and GitLab (what a forge doesn't support is hidden). What's left is marked.

- [x] **A request view in place of the graph** (Esc goes back): a header with its state, branches, checks, approvals and unresolved threads, and **Check out ▾**, **Review** and **Merge**; tabs below it for **Conversation**, **Commits** and **Changes**.
- [x] **Changes:** the request's files (each with its open threads) and its diff with line picking, with comment threads under the lines they're about: reply, resolve and unresolve.
  - [x] Whole file, Split and Blame in a request's diff (from its commits, fetched without a branch).
- [x] **Comments are drafts until the review is finished**, then all sent at once: on a line (its **+**), on a range of lines (click, Shift+click or drag; then right-click › *Comment on selection*), replies, and comments on the whole request. Drafts are marked, can be edited or deleted, and are kept on this computer until sent.
- [x] **Finish review:** lists every draft (click one to go to it), an optional summary, and **Comment**, **Approve** or **Request changes** (GitHub only: GitLab's API has no way to ask for changes); "Submit review and send 3" posts everything together.
- [x] **Conversation:** the description and comments as formatted Markdown, threads with the code they're about (*Show in Changes*), check results, reviews and comments, in order; a comment box at the bottom (a draft too).
- [x] **Commits:** the request's commits; click one to see it in the graph.
  - [x] Marked in the graph (*Show them in the graph*, with a bar back to the request).
- [x] **Merge:** Merge commit, Squash or Rebase (each explained; only what the repository allows), the commit message, *Delete the branch after merging*. When it can't be merged yet, it says why. Afterwards: a *Merged* banner, and *Pull to get it*.
- [x] **Check out ▾:** the request's branch (following the request, so Pull brings new commits; uncommitted changes in the way get the usual offer to put them aside), or a **new worktree** beside the repository. Once on it: *Pull the latest* and *Back to main*. A request from a fork gets a branch of its own.
  - [x] The new worktree opens in a tab; uncommitted changes in the way: put them aside (they come back with *Back to…*), use a worktree instead, or cancel.
- [x] **Suggested changes:** shown as what they'd change; **Apply** (GitLab, one at a time or several in one commit) or **Apply here** on the checked-out branch (GitHub, whose API can't); *Suggest a change* on picked lines.
- [ ] **The active profile's account** does the reviewing and merging (waits on Profiles); when it can't, say so up front.
- [x] **Opening a request:**
  - [x] The target branch is a **dropdown** of the remote's branches, starting on the repository's default branch (not a text field).
  - [x] The commits it will contain; a warning when it won't merge cleanly into the target, or is behind it.
    - [x] Its **Changes** too: the files, each with its diff.
  - [x] From **any branch**: right-click a branch (sidebar or graph) › *Open a merge request…*, as well as the **+** in the sidebar.
    - [x] After pushing a new branch, offer to open one.
  - [x] **Title and description filled in**: one commit → its message; several → the branch name and the commits listed; the repository's template when it has one (a picker for several).
  - [x] **Reviewers, assignees and labels**; GitLab's *Delete source branch* and *Squash when merging*.
  - [x] **Push and open in one step** ("Push 3 commits and open"), setting the upstream; draft or ready; then straight to the new request's view.
- [x] Drafts kept on the forge (GitHub's pending review, GitLab's draft notes); editing and deleting your comments; a status line in the sidebar; outdated threads in Changes; GitLab fast-forward and bringing a request up to date; a new request as a view of its own.
- Later: check logs and re-runs; editing the title, description, reviewers, assignees and labels; draft/ready, close/reopen; sidebar filters (*Mine*, *Review requested*), and a count of requests waiting on you.

### Conflict handling

Mockups: [docs/design/conflicts](docs/design/conflicts) (today's view is `0-today.png`). Build work: the [Conflict handling milestone](https://github.com/Trashpanda-GameDev/Git-Raccoon/milestone/3).

Everything is named by branch or commit (*main*, *feature/login*, "Remember the last user"), never "ours" and "theirs", which swap meaning between a merge and a rebase.

- [ ] **A. One place for the whole conflict**, in place of the banner: what's in progress ("Rebasing feature/login onto main · applying commit 3 of 5: *Remember the last user*"), progress (2 of 4 conflicts resolved, in 3 files), each file with its state (conflicts left; deleted or renamed on one side, with *Keep yours* / *Delete it*; resolved, with *Review* and *Undo*; merged by Git), *Next conflict*, and **Skip this commit**, **Abort** and **Continue** (saying how many are left).
- [ ] **B. A three-way editor:** the common ancestor and both sides (each with the commit, author and age that changed it) above an editable result. Per conflict: *Take main*, *Take feature/login*, *Both ▾* (in either order); per line, a checkbox to take it; editing anywhere; previous/next conflict (Ctrl+↑/↓); *Done with this file*.
- [ ] **C. Warn before conflicting:** before Merge, Rebase and Pull, check without touching any files (`git merge-tree`) and say which files would conflict, with *Rebase instead* (or *Merge instead*) and Cancel. Pull/merge requests show "Conflicts with main in 2 files".
- [ ] **D. Why each side changed it:** beside a conflict, the commits on each side that changed its lines, with their messages and authors (and the request they came in with, if known); *Show these commits* selects them in the graph.
- [ ] **E. An agent's proposal:** agent tools to read the conflicts and propose a file's resolution, with its reasoning; shown in the editor as a proposal to *Use*, *Edit* or *Discard*, written only when the person chooses. Also *Ask an agent* from the conflict screen when one is connected.
- [ ] **F. Solve it once:** Settings › Git › *Remember how I resolve conflicts* (Git's `rerere`); a conflict seen before is resolved the same way and marked as such, for review (*Review*, *Redo by hand*).

## Later (ideas, not commitments)

- Commit message suggestions from the staged diff
- Built-in merge tool for three-way merges
- Workspaces that group related repos
