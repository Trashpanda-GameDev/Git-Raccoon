# Git Raccoon user guide

Git Raccoon is a desktop app for working with Git repositories: you see your project's history as a graph, and do everyday Git work (committing, branching, merging, syncing) with buttons, menus and drag and drop. It runs your own `git` for anything that changes a repository, so your hooks, signing, SSH keys and credential helpers work exactly as on the command line.

- [Installing](#installing)
- [Opening a repository](#opening-a-repository)
- [The window](#the-window)
- [Committing](#committing)
- [Branches and tags](#branches-and-tags)
- [Syncing with a remote](#syncing-with-a-remote)
- [Merging, rebasing and conflicts](#merging-rebasing-and-conflicts)
- [Rewriting history](#rewriting-history)
- [Looking back: file history and blame](#looking-back-file-history-and-blame)
- [Undo](#undo)
- [Tabs, submodules and worktrees](#tabs-submodules-and-worktrees)
- [Keyboard and the command palette](#keyboard-and-the-command-palette)
- [Themes and settings](#themes-and-settings)
- [Updates and crash reports](#updates-and-crash-reports)
- [Troubleshooting](#troubleshooting)

## Installing

Download the installer for your system from the [releases page](https://github.com/Trashpanda-GameDev/Git-Raccoon/releases):

- **Windows:** `GitRaccoon-win-Setup.exe`. It asks where to install Git Raccoon (any folder you can write to, since it keeps itself up to date there), installs for you only (no administrator needed), adds Desktop and Start-menu shortcuts, and uninstalls from *Settings > Apps*. Windows may warn that the installer is from an unknown publisher: choose *More info*, then *Run anyway*.
- **macOS (Apple Silicon):** the `.pkg` installer. The first time, right-click Git Raccoon in Applications and choose *Open* (or allow it in *System Settings > Privacy & Security*).
- **Linux:** the `.AppImage`. Make it executable (`chmod +x`) and run it.

Git Raccoon needs [Git](https://git-scm.com) installed. Once installed, it keeps itself up to date (see [Updates](#updates-and-crash-reports)).

## Opening a repository

The welcome screen offers:

- **Open a repository…**: pick a folder with a Git repository (any folder inside one works too).
- **Clone a repository…**: paste a URL (HTTPS or SSH), choose where it goes, and Git Raccoon clones it with progress and a Cancel button, then opens it.
- **New repository…**: pick a folder to start a repository in.
- **Recent** repositories, one click away.

Your open repositories come back the next time you start Git Raccoon.

## The window

- **The graph** (middle) shows every commit, newest at the top, with lines for branches and merges, and labels for branches, remote branches and tags. A cloud after a branch's name means the remote's copy of it is at the same commit. When a commit has several labels ("main +2"), point at them to see them all. Double-click a label to check that branch out. Click a commit to see its details; use the arrow keys and Page Up/Down to move.
- **The sidebar** (left) lists local branches (with how far ahead or behind their upstream they are), remotes and their branches, tags, stashes, worktrees and submodules. Click one to jump to its commit. The filter box at the top narrows the lists.
- **The details pane** (right) shows the selected commit: its message, author, date, parents and changed files. Click a file to see its diff.
- **Diffs** show unified or side by side, with syntax highlighting, the changed words marked, and images previewed.
- **Search** (Ctrl+F) finds commits by message, author or SHA, or by the files they touch.

The toolbar has the repository menu, the current branch, Fetch, Pull and Push, Undo, the command palette and settings.

## Committing

When you have uncommitted changes, a **// WIP** row sits at the top of the graph. Click it to see your changes:

- **Unstaged** and **staged** files, each with buttons to stage, unstage or discard. Discarded files go to the Recycle Bin or Trash, so they can be recovered.
- Open a file's diff to stage **single hunks**, or pick **individual lines** in the gutter and stage just those.
- Write a **summary** (the counter turns amber past 50 characters and red past 72) and an optional description, then **Commit** (or Ctrl+Enter).
- **Amend last commit** replaces the last commit instead; it warns if that commit is already pushed.
- **Stash** puts all your changes aside; the stash appears in the sidebar, where you can apply, pop or drop it.

Your hooks run as usual. If one refuses the commit, its message is shown and your commit message is kept.

## Branches and tags

Right-click a branch in the sidebar to **check it out**, **create a branch** from it, **rename** it or **delete** it (double-click also checks out). The **+** next to *Local* creates a branch. Right-click a commit in the graph to create a branch or tag there, or check out that commit.

Git Raccoon protects your work: if uncommitted changes are in the way of a checkout, it offers to stash them first; deleting a branch with commits that aren't anywhere else asks twice and tells you how to get them back.

Tags can be lightweight, or annotated (with a message).

## Syncing with a remote

- **Fetch** gets new commits from every remote without changing your branches; the sidebar and toolbar then show how far behind you are.
- **Pull** brings the upstream's new commits into your branch. It only fast-forwards by default; if both sides have new commits, it asks whether to **merge** or **rebase**. Uncommitted changes are put aside and restored.
- **Push** sends your commits. The first push of a new branch sets up its upstream. If the remote has commits you don't, it offers to pull first, or to **force push** — which is always done *with lease*, so it never deletes commits you haven't seen.

Progress shows in the toolbar, with a Cancel button.

**Signing in.** Git Raccoon uses your existing Git setup: Git Credential Manager (included with Git for Windows) and other credential helpers, and keys in your SSH agent. When Git needs a password, a passphrase or confirmation of a new host, Git Raccoon asks in a dialog.

**Remotes** are managed in the sidebar: **+** next to *Remote* adds one (and fetches it); right-click one to edit or remove it.

## Merging, rebasing and conflicts

- **Merge:** right-click a branch and choose *Merge into …*, or **drag** it onto another branch in the sidebar. You can fast-forward when possible or always create a merge commit.
- **Rebase:** right-click a branch and choose *Rebase … onto …* (it warns if your commits are already pushed).

If changes conflict, a banner says a merge (or rebase) is in progress, and the conflicted files open:

- Each conflict shows both versions. Choose **Use ours**, **Use theirs** or **Use both**, or edit the file yourself with **Edit**.
- **Use ours / theirs for the file** resolves a whole file at once.
- When no conflict markers remain, **Mark resolved**.
- When every file is resolved, **Continue** in the banner. **Abort** goes back to how things were.

## Rewriting history

Right-click a commit in the graph:

- **Cherry-pick onto …** copies its changes onto your branch.
- **Revert this commit** adds a commit that undoes it.
- **Reset … to here** moves your branch back: *soft* keeps the changes staged, *mixed* keeps them in your files, *hard* discards them.
- **Interactive rebase from here** opens a planner for the commits after it: drag them to reorder, and choose for each **Pick**, **Reword**, **Squash**, **Fixup** or **Drop**. Then **Start rebase**.

## Looking back: file history and blame

In any diff, **History** lists every commit that changed the file (following renames), with each one's change beside the list. **Blame** shows, for every line, the commit and author that last changed it; click one to go to that commit.

## Undo

After most actions, the toolbar shows exactly what **Undo** would do ("Undo commit 'Fix the thing'"), and Ctrl+Z does it: commits, amends, checkouts, merges, rebases, cherry-picks, reverts, resets, branch and tag changes, and stash save and drop. An undo first checks nothing has changed since, so it never throws away newer work.

## Tabs, submodules and worktrees

- Each repository opens in its own **tab**. Ctrl+T opens a new tab, Ctrl+W closes one, Ctrl+Tab moves between them. Tabs in the background keep working.
- **Submodules** are listed in the sidebar with their state; right-click to open one in a tab, or update it.
- **Worktrees** (extra working folders of the same repository) are listed too: **+** adds one on a new or existing branch; right-click to open or remove one.
- Repositories using **Git LFS** show an *LFS* badge (amber if git-lfs isn't installed), and diffs of LFS files show their sizes.

## Keyboard and the command palette

**Ctrl+K** opens the command palette: type a few letters to find any action, or a branch to check out, merge, rebase onto or go to. Each line shows its shortcut. (On macOS, use Cmd for Ctrl.)

| Action | Shortcut |
| --- | --- |
| Command palette | Ctrl+K |
| Settings | Ctrl+, |
| Open repository | Ctrl+O |
| New tab / Close tab | Ctrl+T / Ctrl+W |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab |
| Search commits | Ctrl+F |
| Undo | Ctrl+Z |
| Fetch / Pull / Push | Ctrl+Shift+F / D / U |
| Show working changes | Ctrl+Shift+W |
| Write a commit message | Ctrl+Shift+M |
| Stage all / Unstage all | Ctrl+Shift+S / A |
| Stash | Ctrl+Shift+H |
| New branch / New tag | Ctrl+Shift+B / T |
| Refresh | F5 |

## Themes and settings

**Settings** (the gear, or Ctrl+,) has:

- **Theme:** Raccoon (the default), System (light or dark, following your computer), Dark, Light, Pastel, Abyss, Forest, Paper, or High contrast.
- **Interface size**, from 90% to 150%.
- **Diffs:** unified or side by side.
- **Updates:** whether to check automatically.

## Updates and crash reports

**Updates.** An installed Git Raccoon checks for a new version now and then and downloads it in the background. When it's ready, the toolbar shows **Restart to update**; your tabs and settings are kept.

**Crash reports.** If Git Raccoon runs into a bug, it writes a report on your computer — nothing is sent. Next time you start it, it shows you the report and lets you choose: **Report on GitHub…** opens an issue you can read and add to before posting, **Copy report**, or **Don't send**.

## Troubleshooting

- **"Git isn't installed, or isn't on PATH."** Install [Git](https://git-scm.com), then restart Git Raccoon.
- **Fetching or pushing asks for a password every time.** Set up a credential helper (Git Credential Manager comes with Git for Windows), or use SSH with your key in an SSH agent.
- **"This host isn't known yet."** Git Raccoon asks you to confirm a new SSH host the first time; check its fingerprint against the one the host publishes.
- **Large files show as small text files.** The repository uses Git LFS: install [git-lfs](https://git-lfs.com) and run `git lfs pull`.
- **Something went wrong.** The error banner shows Git's own message. If it looks like a bug, a report on the project's [issues page](https://github.com/Trashpanda-GameDev/Git-Raccoon/issues) helps.
