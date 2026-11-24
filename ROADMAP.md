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

## Conflict handling (0.5.25, 0.5.26)

Designed in [docs/design/conflicts](docs/design/conflicts); built in the [Conflict handling milestone](https://github.com/Trashpanda-GameDev/Git-Raccoon/milestone/3). Everything is named by branch or commit (*main*, *feature/login*), never "ours" and "theirs".

- [x] **One place for the whole conflict**, in place of the history: what's in progress and how far along (step 3 of 5 of a rebase), each file with what it needs (*Resolve…*; *Keep main's* / *Delete it* when deleted on one side; resolved files with *Review* and *Undo*), *Next conflict*, and **Skip this commit**, **Abort** and **Continue** (saying how many are left)
- [x] **A three-way editor:** what both sides started from and each side's version, with the commit that changed it; per conflict *Take main*, *Take feature/login*, *Both ▾* (either order); ticking single lines, which go into the result in the order ticked; an editable result; Ctrl+↑/↓ between conflicts
- [x] **Panes:** each side's whole file side by side with the output below, taking the whole window; tick a conflict or single lines, *Save* writes every chosen conflict at once
- [x] **Warn before conflicting:** Merge and Rebase say which files would conflict, worked out without touching any files; a merging or rebasing Pull asks first when it would conflict (as of the last fetch). Pull/merge requests already say when they conflict, from the forge.
- [x] **Why each side changed it:** the commits on each side that changed the conflict's lines, with their messages, authors and the request named in their message; *Show these commits* marks them in the graph; a hint when it's clear what happened (a rename, lines only added)
- [x] **An agent's proposal:** agent tools to read the conflicts and propose a file's resolution with its reasoning, shown as a proposal to *Use this* (undoable), *Edit it* or *Discard*; *Ask an agent* only while one is connected
- [x] **Solve it once:** Settings › Git › *Remember how I resolve conflicts* (Git's `rerere`, for the app's Git only); a conflict seen before comes back resolved, marked *resolved as before*, with *Review* and *Redo by hand*, and still needs *Mark resolved*

## SSH setup (0.5.28)

Designed in [docs/design/ssh](docs/design/ssh); built in the [SSH setup milestone](https://github.com/Trashpanda-GameDev/Git-Raccoon/milestone/4). Throughout: the user's own `~/.ssh/config`, agent and `core.sshCommand` win; nothing outside `~/.ssh` and the repository's remotes is changed, and nothing without saying so first.

- [x] **SSH status:** the keys in `~/.ssh` and those `~/.ssh/config` names (type, fingerprint, comment, passphrase or not); the agent (which one, running, its keys); what `ssh -G` would use for a host; a connection test that says who you're signed in as, or exactly why not
- [x] **Settings › SSH:** your keys, each in the agent or not and on which signed-in accounts (*Copy public key*, *Add to the agent*, *Add to GitHub/GitLab*, *Show in folder*); *Create a key…* and *Use an existing key…*; the agent and *Add keys to the agent when first used*; trusted hosts and *Test connections*
- [x] **Creating a key** in-process (ed25519, passphrase or not), written to `~/.ssh` privately, never overwriting a key, with an optional `~/.ssh/config` entry shown first
- [x] **Keys on your accounts:** listed and added through GitHub, GitLab and self-managed GitLab; GitHub's sign-in asks for `write:public_key`, and older sign-ins are offered a fresh one when needed
- [x] **Guided setup** (*Set up SSH for GitHub/GitLab*, from Settings, the clone dialog, a remote's menu or an SSH error): key, agent, account, test, and the repository's remote; steps done are ticked and skipped, each can be done by hand
- [x] **Errors that know what's wrong:** a failed fetch, pull or push over SSH is diagnosed (key, agent, account, host trusted, reachable) with the fixes as buttons; background fetches say so quietly. Clones that fail over SSH still show Git's words, with a hint to Settings › SSH.
- [x] **Trusting hosts:** checked against GitHub's published keys (its API, else built in) and GitLab.com's (built in, from its documentation); a changed key is explained, and replacing it offered only when the new one is published
- [x] **SSH or HTTPS:** the clone dialog's choice (SSH when your key already works), a remote's *Use SSH/HTTPS instead*, *Use port 443* and *Test connection*, with undo
- [x] **Windows:** the app's Git uses Windows' `ssh` when Windows' agent has the keys (its own commands only); *Use it for Git everywhere too*; how to turn the agent service on. The detection runs in CI on Windows.
- [ ] Tried end to end on Windows, macOS and Linux with real GitHub and GitLab accounts (a new key, a key in another agent, a blocked port 22)
- Later (with Profiles, now built): a key per profile (two accounts on one host, through host aliases); signing commits with an SSH key.

## Profiles (0.5.29)

Mockup and clickable prototype: [Git Raccoon profiles](https://claude.ai/artifact/EHXRYoBeVSrxSwNXLSaveU). Built in the [Profiles milestone](https://github.com/Trashpanda-GameDev/Git-Raccoon/milestone/1).

- [x] **A profile** has a name, a colour, the name and email to commit as, and the forge accounts to use (at most one per forge, so it's always clear which account pushes).
- [x] **Accounts aren't owned by a profile.** They're signed in once, under Settings › Accounts, and any number of profiles can use the same one (one GitHub account with a work and a personal email is the common case). Each profile's editor says which other profiles use an account; signing out of one says which profiles lose it.
- [x] **One profile is active for the whole app**, picked from a coloured chip at the top right of the toolbar. Its menu lists each profile with its name, email and accounts, plus *New profile…* and *Manage profiles…*.
- [x] **Settings › Profiles** (above Accounts): the list of profiles, and the chosen one's name, colour, commit name and email, and its accounts (ticking a second account for the same forge swaps it in). *Sign in to another account…* adds an account to that profile. Delete, duplicate, *Use this profile*; saved as you type.
- [x] **The commit box** says who you're committing as (click to switch), and the button says "Commit 3 files as Work".
- [x] **A warning** when the repository's remote belongs to an account that only another profile has ("…an account in your Personal profile. Switch to Personal"), or that no profile has. When the account is in several profiles, no warning.
- [x] **Commits use the profile's name and email** for Git Raccoon's own commits, leaving the user's Git config alone.
- [x] **First run:** one "Default" profile from the current Git name and email, holding every signed-in account, so nothing changes until a second profile is added.
- Later: remember a profile per repository, or rules by remote (`github.com/acme/*` → Work) or folder; commit signing per profile; an SSH key per profile.
- [ ] Tried end to end with real GitHub and GitLab accounts: two profiles sharing one account, and one each.

## Find which commit broke something (0.5.30)

Mockups: [docs/design/bisect](docs/design/bisect). Built in the [Find which commit broke something milestone](https://github.com/Trashpanda-GameDev/Git-Raccoon/milestone/5).

Git's bisect, in plain words: "broken" and "works", not "bad" and "good" or "bisect". It runs through the user's own `git bisect`, so a search started in a terminal and one started here are the same search.

- [x] **Bisect underneath:** start (with a broken and a working commit, optionally `--first-parent`), answer, skip, undo the last answer (replaying Git's log), stop; read a search in progress from the repository (its range, answers and current commit); how many commits and steps are left; the result.
- [x] **A. Starting:** right-click a commit › *Find which commit broke something…* (it's the broken one); *Last worked at* suggests the latest tag, or any commit; the range and steps ("13 commits, about 4 steps"); *Only follow main*; uncommitted changes put aside first and brought back at the end.
- [x] **B. By hand:** a bar with what's being tested and what's left, and **It works**, **It's broken**, **Skip this one** (for a commit that can't be tested), **Stop**; the graph marks the range still in question, the answers, and the commit checked out; *Undo last answer* and *Open a terminal here*.
- [x] **C. With a command:** *Test with a command…* runs it on each commit (read as `git bisect run` does: exit 0 works, 125 skip, anything else broken), with each step's result, time and output, progress, and Stop; the command remembered per repository.
- [x] **D. The answer:** the first broken commit (message, author, files, the request it came with), how it was found, and *Show it in the history*, *Revert it…*, *Create a branch here…*, *Copy a summary*, *Ask an agent why*; back where the search started, with the changes put aside brought back. When skips leave it uncertain, the commits it could be.
- [x] **Picking up:** a search in progress (started in a terminal, or before the app was closed) opens in the same view; *Stop* always goes back to where it started.
- [x] **Other questions:** custom terms for "when did this change?" ("Old behaviour" / "New behaviour", or your own words).
- [x] **Agents:** tools to start a search, answer for the commit being tested, run a command, and read where it stands, with the same approval as other changes.
- [x] Also: an optional time limit per step (a step that runs over is skipped), and operations that would move away from the commit being tested (checkout, commit, merge, rebase…) say a search is going on, with *Stop* first.
- [ ] Tried on Windows, macOS and Linux on a real project, by hand and with a test command.

## Designed (ready to build)

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
- [x] **The active profile's account** does the reviewing and merging.
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

## Later (ideas, not commitments)

- Commit message suggestions from the staged diff
- Workspaces that group related repos
