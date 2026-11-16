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
- **Clone a repository…**: paste a URL (HTTPS or SSH), choose where it goes, and Git Raccoon clones it with progress and a Cancel button, then opens it. Signed in to GitHub or GitLab (see [Pull requests, merge requests and issues](#pull-requests-merge-requests-and-issues))? Pick the account under **From** and choose one of your repositories from the list (type to find it), or click **New repository on …** to make a new one there (public or private) and clone it straight away.
- **New repository…**: pick a folder to start a repository in.
- **Recent** repositories, one click away.

Your open repositories come back the next time you start Git Raccoon.

## The window

- **The graph** (middle) shows every commit, newest at the top, with lines for branches and merges, and labels for branches, remote branches and tags. A cloud after a branch's name means the remote's copy of it is at the same commit. When a commit has several labels ("main +2"), point at them to see them all. A label too long for its column grows to its whole name when you point at it. Double-click a label to check that branch out. Double-clicking a remote branch (such as `origin/feature`) checks out your branch of the same name and brings it up to that commit; if both have new commits, it asks whether to merge or rebase. Click a commit to see its details; use the arrow keys and Page Up/Down to move.
- **The sidebar** (left) lists local branches (with how far ahead or behind their upstream they are), remotes and their branches, tags, stashes, worktrees and submodules. Click one to jump to its commit. The filter box at the top narrows the lists. Every section's heading stays in view: a long list scrolls inside its own section, and you can drag the line under a section to make it taller or shorter (double-click the line to let it fit its contents again).
- **Sizes and columns:** drag the edge of the sidebar or the details pane to make it as wide or narrow as you like. Drag a column heading in the graph (Graph, Commit message, Author…) left or right to move that column; right-click a heading to put them back in their usual order.
- **The details pane** (right) shows the selected commit: its message, author, date, parents and changed files. Click a file to see its diff.
- **Diffs** show unified or side by side, with syntax highlighting, the changed words marked, and images previewed.
- **Search:** press Ctrl+F for the search bar, to find commits by message, author or SHA, or by the files they touch. Enter and Shift+Enter go through the matches; Esc closes it.

The toolbar has the repository menu (your recent repositories, to switch to one), the current branch, Branch (a new branch from the checked-out commit), Fetch, Pull and Push, Undo, the command palette and settings.

## Committing

When you have uncommitted changes, a **// WIP** row sits at the top of the graph. Click it to see your changes:

- **Unstaged** and **staged** files, each in a list of its own (so staged files stay in view, however many unstaged ones there are; click a list's heading to fold it away), with buttons to stage, unstage or discard. Pick several files with Ctrl+click (or Shift+click for a range) and right-click them, or right-click a folder in the folder view, to stage, unstage, stash, ignore (new files: they're added to `.gitignore`) or discard them all at once. The tree button at the top shows the files in their folders instead of by path (commits and stashes too; a folder holding just one folder shares its row, like `game/assets/localisation`), and the bin discards all changes, staged or not, after asking. Discarded files go to the Recycle Bin or Trash, so they can be recovered.
- Open a file's diff to stage **single hunks**, or pick **individual lines** in the gutter and stage just those.
- Write a **summary** (the counter turns amber past 50 characters and red past 72) and an optional description, then **Commit** (or Ctrl+Enter).
- **Amend last commit** replaces the last commit instead; it warns if that commit is already pushed.
- **Stash** puts your changes aside: all of them, or, when some are staged, just the staged ones (the rest stay as they are). The stash appears in the sidebar, and in the graph just above the commit it was made on; right-click it in either to apply, pop or delete it.

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

**Several branches at once:** Ctrl+click (Cmd+click on a Mac) branches in the sidebar to pick them, then right-click one to delete them all (local ones, or remote ones on their remote). It asks once, and if any has work that isn't merged anywhere, it asks again before deleting any of them. Undo brings them all back; Esc unpicks.

**Remote branches** can be deleted on the remote: right-click one under *Remote* and choose *Delete on the remote*. It asks first, as it's gone for everyone; Undo puts it back.

**Remotes** are managed in the sidebar: **+** next to *Remote* adds one (and fetches it); right-click one to edit or remove it.

## Pull requests, merge requests and issues

For a repository on **GitHub**, **GitLab.com** or a **self-managed GitLab**, the sidebar lists its open pull requests (merge requests on GitLab) and issues, under the branches. Git Raccoon finds the forge from the repository's remote (*origin* first).

**Signing in.** Open **Settings → Accounts → Add account…** (or click *Sign in* in the sidebar section), and pick GitHub, GitLab.com or Self-managed GitLab (give its address, like `https://gitlab.example.com`). With the browser, Git Raccoon shows a short code and opens the forge's page to approve it. Or click **Use a token instead**, then **Make a token…**: the forge's page opens with the right permissions ticked (`repo` and `read:org` on GitHub, `api` on GitLab); create it and paste it in. You can be signed in to several at once (GitHub, GitLab.com and any number of servers); each repository uses the account for its remote.

**A self-managed GitLab with the browser.** The server needs Git Raccoon registered as an application once: choose *Use the browser instead*, click **Make an app…** (it opens *Applications* in your GitLab settings), give it any name and redirect URI, untick *Confidential*, tick the `api` scope, and paste its **Application ID** into the sign-in window. Git Raccoon remembers it for that server. (This needs a GitLab version with device sign-in, 17.2 or later.) Tokens are kept in your system's keychain (Keychain on macOS, Credential Manager on Windows, the Secret Service on Linux), never in the settings file. **Sign out** in the same place forgets it.

**A request's details.** Click one to see who wants to merge what into what, how its reviews and checks (CI) stand, its comments and description.

- **Check out** puts you on its branch, brought up to the remote's. A request from a fork is fetched into a branch of its own (`pr/12` or `mr/12`).
- **Open on GitHub/GitLab** opens it in the browser.

**A new request.** The **+** next to *Pull requests* (or *Merge requests*) opens one from the checked-out branch: pick the branch it goes into, give it a title and a description, and whether it's a draft. If the branch has commits the remote doesn't, push them first (there's a Push button right there).

**Open on GitHub/GitLab.** Right-click a commit, a branch (a local one opens its upstream), a remote, a tag, a pull/merge request or an issue to open its page in the browser, or to copy its link. This works for any remote on GitHub or GitLab.com, and for self-managed GitLab servers you've signed in to; no sign-in is needed for the pages themselves.

**Issues** open in the details pane the same way. Esc closes what's open. The lists reload when you fetch or refresh.

## Merging, rebasing and conflicts

- **Merge:** right-click a branch and choose *Merge into …*, or **drag** it onto another branch, in the sidebar or by its label on the graph. You can fast-forward when possible or always create a merge commit.
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

After most actions, the toolbar shows exactly what **Undo** would do ("Undo commit 'Fix the thing'"), and Ctrl+Z does it: commits, amends, checkouts, merges, rebases, cherry-picks, reverts, resets, branch and tag changes, and stash save and delete. An undo first checks nothing has changed since, so it never throws away newer work.

## Tabs, submodules and worktrees

- Each repository opens in its own **tab**. Ctrl+T opens a new tab, Ctrl+W closes one, Ctrl+Tab moves between them; drag a tab to put it somewhere else. Tabs in the background keep working.
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
| Zoom in / out / actual size | Ctrl++ / Ctrl+- / Ctrl+0 (or Ctrl+scroll) |
| Refresh | F5 |
| Open a terminal here | Ctrl+` |

**A terminal.** For anything Git Raccoon doesn't do, the terminal button in the toolbar (or Ctrl+`, or "Open a terminal here" in the command palette) opens a terminal in the repository's folder: Windows Terminal, Git Bash or the Command Prompt on Windows, Terminal on macOS, and on Linux the one in `$TERMINAL` or the first of the usual ones that's installed. Worktrees and submodules have it in their right-click menu too. To use another terminal, set it in Settings.

## Themes and settings

**Settings** (the gear, or Ctrl+,) has:

- **Theme:** a look (Raccoon, the default, Classic, Pastel, Forest, Paper or High contrast), each **Dark** or **Light**, or **System** to follow your computer's light and dark.
- **Interface size**, from 90% to 150%. Anywhere in the app, Ctrl+scroll or Ctrl+plus and Ctrl+minus zoom in and out (from 50% to 200%), and Ctrl+0 goes back to normal; the size is remembered.
- **Diffs:** unified or side by side.
- **Terminal:** the command "Open a terminal here" runs, if not the system's own; `{path}` stands for the repository's folder (for example `alacritty --working-directory "{path}"`).
- **Updates:** whether to check automatically.

## Updates and crash reports

**Updates.** An installed Git Raccoon checks for a new version now and then and downloads it in the background. When it's ready, the toolbar shows **Restart to update**; your tabs and settings are kept.

**What's new.** After an update, Git Raccoon shows what changed since the version you had (and "What's new in this version" in the command palette shows it again).

**Feedback.** The speech-bubble button in the toolbar (or "Send feedback" in the command palette) takes a bug report, feedback or an idea: write a title and a few details and press **Send**: it's posted as a public issue on Git Raccoon's GitHub page, for the project's board. No account is needed, and nothing else about you is sent; bug reports include the version and system. Tick **Include a screenshot of the window** to send what the window looked like when you opened the form; it's shown to you first, as it's public too.

**Crash reports.** If Git Raccoon runs into a bug, it writes a report on your computer — nothing is sent. Next time you start it, it shows you the report and lets you choose: **Report on GitHub…** opens an issue you can read and add to before posting, **Copy report**, or **Don't send**.

## Troubleshooting

- **"Git isn't installed, or isn't on PATH."** Install [Git](https://git-scm.com), then restart Git Raccoon.
- **Fetching or pushing asks for a password every time.** Set up a credential helper (Git Credential Manager comes with Git for Windows), or use SSH with your key in an SSH agent.
- **"This host isn't known yet."** Git Raccoon asks you to confirm a new SSH host the first time; check its fingerprint against the one the host publishes.
- **Large files show as small text files.** The repository uses Git LFS: install [git-lfs](https://git-lfs.com) and run `git lfs pull`.
- **Something went wrong.** The error banner shows Git's own message. If it looks like a bug, a report on the project's [issues page](https://github.com/Trashpanda-GameDev/Git-Raccoon/issues) helps.
