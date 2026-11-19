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
- **Clone a repository…**: give its **URL** (HTTPS or SSH). Or choose one of your GitHub or GitLab accounts at the top, and pick one of your repositories there from a list you can type in to narrow it down; that fills in the URL (see [Pull requests, merge requests and issues](#pull-requests-merge-requests-and-issues) for signing in). Choose where it goes and, if you like, a **Shallow clone** (the latest commit only, not the history before it) or a **Sparse checkout** (only the folders you name, and the files at the top). Git Raccoon clones it with progress and a Cancel button, then opens it.
- **New repository…**: it's made only on this computer, unless you choose one of your GitHub or GitLab accounts at the top: then it's made there too, and you can give a description, choose **Public** or **Private**, and whether to clone it here after creating it. Give its name, the folder it goes in (its own folder, named after it, is made there), its first branch's name, and optionally a **.gitignore template** and a **License** (from the forge's list; GitHub's for one only on this computer) and **Git LFS**. Those go in a first commit, which is pushed to the forge when it's made there.
- **Recent** repositories, one click away.

Your open repositories come back the next time you start Git Raccoon.

## The window

- **The graph** (middle) shows every commit, newest at the top, with lines for branches and merges, and labels for branches, remote branches and tags. A cloud after a branch's name means the remote's copy of it is at the same commit. When a commit has several labels ("main +2"), point at them to see them all. A label too long for its column grows to its whole name when you point at it. Double-click a label to check that branch out, or right-click it for everything its menu in the sidebar has (merge, rebase, rename, delete…) along with what can be done with its commit. Double-clicking a remote branch (such as `origin/feature`) checks out your branch of the same name and brings it up to that commit; if both have new commits, it asks whether to merge or rebase. Click a commit to see its details; use the arrow keys and Page Up/Down to move. Shift+↓/↑ follows the selected commit's branch (down to its parent, up to the commit that continues it), Ctrl+↓/↑ goes to the next or previous commit with a label, Home and End to the newest and oldest, and Ctrl+H to the checked-out commit. When there are more branches side by side than the graph's column shows, it scrolls sideways: use the horizontal wheel (or Shift and the wheel) over it, or drag the bar under the Graph heading.
- **The sidebar** (left) lists local branches (with how far ahead or behind their upstream they are), remotes and their branches, tags, stashes, worktrees and submodules. Click one to jump to its commit. The filter box at the top narrows the lists. The open sections share the sidebar's height between them, and a long list scrolls inside its own section. Drag the line between two sections to move just that boundary (double-click it to share the space by what's in each again). Click a heading to collapse its section: its heading stays in its place, on top of the next section, and the open ones take its space. Right-click any heading to hide sections you don't use (pull or merge requests, issues, tags…) or show them all again; the same switches are in Settings › Appearance › Sidebar.
- **Sizes and columns:** drag the edge of the sidebar or the details pane to make it as wide or narrow as you like. Drag a column heading in the graph (Graph, Commit message, Author…) left or right to move that column: the others make room as you go. Drag the edge after a heading to make that column wider or narrower (the commit message takes the room that's left, and no column gets narrower than its heading), or double-click the edge to fit the column to what's on screen. Right-click a heading to put the order or the widths back as they were.
- **The details pane** (right) shows the selected commit: its message, author, date, parents and changed files. Click a file to see its diff.
- **Diffs** show unified or side by side, just the changes (with a few lines around them) or the **whole file** with the changes marked in place (it opens at the first change), with syntax highlighting, the changed words marked, and images previewed. Long lines scroll sideways: the scrollbar along the bottom, or the horizontal wheel (or Shift and the wheel) over the diff. Side by side, each half has its own scrollbar, the two halves move together, and the line numbers stay put.
- **Search:** press Ctrl+F, or **Search** in the toolbar, for the search box over the top right of the graph, to find commits by message, author or SHA, or (with the file button) by the files they touch. Enter and Shift+Enter, or the arrows, go through the matches, and the box shows which one you're on ("3 of 40"); Esc closes it.

The toolbar has the repository menu (your recent repositories, to switch to one), the current branch, Branch (a new branch from the checked-out commit), Fetch, Pull and Push, Undo, the command palette and settings.

## Committing

When you have uncommitted changes, a **// WIP** row sits at the top of the graph, with a dashed line down to the commit you're on. You can write the commit's summary right there: it's the same text as the summary box under Commit, and a stash starts with it as its title. Click the row to see your changes:

- **Unstaged** and **staged** files, each in a list of its own, sharing the pane half and half (drag the line between them to share it differently; double-click it for half each again; click a list's heading to fold it away and give its room to the other), with buttons to stage, unstage or discard. Pick several files with Ctrl+click (or Shift+click for a range) and right-click them, or right-click a folder in the folder view, to stage, unstage, stash, ignore (new files: they're added to `.gitignore`) or discard them all at once. The tree button at the top shows the files in their folders instead of by path (commits and stashes too; a folder holding just one folder shares its row, like `game/assets/localisation`), and the bin discards all changes, staged or not, after asking. Discarded files go to the Recycle Bin or Trash, so they can be recovered.
- Open a file's diff to stage **single hunks**, or pick **individual lines** and stage just those: click a line, Shift+click or drag for a range, Ctrl+click to add or take one out, then right-click them and choose **Stage selection** (only the changed lines among them count). Right-click → **Copy** copies picked lines (as does Ctrl+C), in any diff, or picks every line. Esc lets go of them.
- Write a **summary** (the counter turns amber past 50 characters and red past 72) and an optional description, then **Commit** (or Ctrl+Enter).
- **Amend last commit** replaces the last commit instead; it warns if that commit is already pushed.
- **Stash** puts your changes aside: all of them, or, when some are staged, just the staged ones (the rest stay as they are). The stash appears in the sidebar, and in the graph just above the commit it was made on; right-click it in either to apply, pop or delete it.

Your hooks run as usual. If one refuses the commit, its message is shown and your commit message is kept.

## Branches and tags

Right-click a branch in the sidebar to **check it out**, **create a branch** from it, **rename** it, **set its upstream** (the remote branch it pulls from and pushes to, or none) or **delete** it (double-click also checks out). The **+** next to *Local* creates a branch. Right-click a commit in the graph to create a branch or tag there, or check out that commit. **Delete…** on a branch or tag (in the sidebar or on the graph) asks where, with a checkbox for each place: *Local*, and each remote (for a branch, the remotes that have it); the one you right-clicked starts ticked. Deleting a branch here and on a remote first checks its commits are on some other branch, and asks again if not. Undo puts it back wherever it went, the remotes included.

Git Raccoon protects your work: if uncommitted changes are in the way of a checkout, it offers to stash them first; deleting a branch with commits that aren't anywhere else asks twice and tells you how to get them back.

Tags can be lightweight, or annotated (with a message).

## Syncing with a remote

- **Fetch** gets new commits from every remote without changing your branches; the sidebar and toolbar then show how far behind you are. Git Raccoon also fetches on its own in the background, every 10 minutes to start with (Settings › Git › *Fetch automatically*: 1 to 60 minutes, or 0 for never); it never asks for a password there, so a remote that needs one is left for you to fetch, and the Fetch button's tip says when it last fetched. The arrow beside it fetches from just one remote (when there are several), and sets how Fetch goes (remembered): **Prune** removes remote branches that were deleted there (on to start with), and **Force** lets fetched refs overwrite local ones even where that would lose commits; a forced fetch asks first.
- **Pull** brings the upstream's new commits into your branch. It only fast-forwards by default; if both sides have new commits, it asks whether to **merge** or **rebase**. Uncommitted changes are put aside and restored. The arrow beside it pulls from another remote (its branch of the same name, when there are several), and sets how Pull goes (remembered): when both sides have new commits, **fast-forward only** (asking, as above), **fast-forward if possible, else merge**, or **rebase**; and uncommitted changes are **put aside and back**, **left** (Git refuses if they're in the way, and offers to stash them), or **discarded** (it asks first each time, and their current versions go to the Recycle Bin).
- **Push** sends your commits. Pushing a branch that isn't on a remote yet asks where it should go, in a bar under the toolbar: the same name on `origin` to start with, or pick another remote (when there are several) or name. From then on it pushes to and pulls from there. If the remote has commits you don't, it offers to pull first, or to **force push** — which is always done *with lease*, so it never deletes commits you haven't seen.

Progress shows in the toolbar, with a Cancel button.

**Signing in.** Git Raccoon uses your existing Git setup: Git Credential Manager (included with Git for Windows) and other credential helpers, and keys in your SSH agent. When Git needs a password, a passphrase or confirmation of a new host, Git Raccoon asks in a dialog. If you're signed in to the repository's forge in Git Raccoon (see below), that account answers for its host instead, with its token.

**Several branches at once:** Ctrl+click (Cmd+click on a Mac) branches in the sidebar to pick them, or click one and Shift+click another to pick every branch shown between them (with Ctrl too, added to those already picked), then right-click one to delete them all (local ones, or remote ones on their remote). It asks once, and if any has work that isn't merged anywhere, it asks again before deleting any of them. Undo brings them all back; Esc unpicks.

**Remote branches** can be deleted on the remote: right-click one under *Remote* and choose *Delete on the remote*. It asks first, as it's gone for everyone; Undo puts it back.

**Remotes** are managed in the sidebar: **+** next to *Remote* adds one (and fetches it); right-click one to edit or remove it.

## Pull requests, merge requests and issues

For a repository on **GitHub**, **GitLab.com** or a **self-managed GitLab**, the sidebar lists its open pull requests (merge requests on GitLab) and issues, under the branches. Git Raccoon finds the forge from the repository's remote (*origin* first).

**Signing in.** Open **Settings → Accounts → Add account…** (or click *Sign in* in the sidebar section), and pick GitHub, GitLab.com or Self-managed GitLab (give its address, like `https://gitlab.example.com`). With the browser, Git Raccoon shows a short code and opens the forge's page to approve it. Or click **Use a token instead**, then **Make a token…**: the forge's page opens with the right permissions ticked (`repo`, `read:org` and `workflow` on GitHub, `api` on GitLab); create it and paste it in. You can be signed in to several at once (GitHub, GitLab.com and any number of servers); each repository uses the account for its remote.

**A self-managed GitLab with the browser.** The server needs Git Raccoon registered as an application once: choose *Use the browser instead*, click **Make an app…** (it opens *Applications* in your GitLab settings), give it any name and redirect URI, untick *Confidential*, tick the `api` scope, and paste its **Application ID** into the sign-in window. Git Raccoon remembers it for that server. (This needs a GitLab version with device sign-in, 17.2 or later.) Tokens are kept in your system's keychain (Keychain on macOS, Credential Manager on Windows, the Secret Service on Linux), never in the settings file. **Sign out** in the same place forgets it.

**A request's details.** Click one to see who wants to merge what into what, how its reviews and checks (CI) stand, its comments and description.

- **Check out** puts you on its branch, brought up to the remote's. A request from a fork is fetched into a branch of its own (`pr/12` or `mr/12`).
- **Open on GitHub/GitLab** opens it in the browser.

**A new request.** The **+** next to *Pull requests* (or *Merge requests*) opens one from the checked-out branch: pick the branch it goes into from the remote's branches (the default one is already picked), give it a title and a description, and whether it's a draft. If the branch has commits the remote doesn't, push them first (there's a Push button right there).

**Open on GitHub/GitLab.** Right-click a commit, a branch (a local one opens its upstream), a remote, a tag, a pull/merge request or an issue to open its page in the browser, or to copy its link. This works for any remote on GitHub or GitLab.com, and for self-managed GitLab servers you've signed in to; no sign-in is needed for the pages themselves.

**Issues** open in the details pane the same way. Esc closes what's open. The lists reload when you fetch or refresh.

## Merging, rebasing and conflicts

- **Merge:** right-click a branch and choose *Merge into …*, or **drag** it onto another branch, in the sidebar or by its label on the graph. You can fast-forward when possible or always create a merge commit.
- **Rebase:** right-click a branch and choose *Rebase … onto …* (it warns if your commits are already pushed).

If changes conflict, a banner says a merge (or rebase) is in progress, and the conflicted files open:

- Each conflict shows both versions. Choose **Use ours**, **Use theirs** or **Use both**, or edit the file yourself with **Edit**.
- **Use ours / theirs for the file** resolves a whole file at once. It asks first, as it replaces anything picked or edited in that file.
- When no conflict markers remain, **Mark resolved**.
- When every file is resolved, **Continue** in the banner. **Abort** goes back to how things were.

## Rewriting history

Right-click a commit in the graph:

- **Cherry-pick onto …** copies its changes onto your branch.
- **Revert this commit** adds a commit that undoes it.
- **Reset … to here** moves your branch back: *soft* keeps the changes staged, *mixed* keeps them in your files, *hard* discards them.
- **Interactive rebase from here** opens a planner for the commits after it: drag them to reorder, and choose for each **Pick**, **Reword**, **Squash**, **Fixup** or **Drop**. Then **Start rebase**.

## Looking back: file history and blame

In any diff, **History** and **Blame** are switches, and can be on together (they're remembered). **History** lists every commit that changed the file (following renames) down the side of the diff; click one to see its change to the file. **Blame** adds a column beside the lines showing the commit and author that last changed each one, with a stripe that's stronger the newer the change; it's most useful with **Whole file**. Click a commit in it to go to that commit.

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
| Follow the branch down / up | Shift+↓ / Shift+↑ |
| Next / previous labelled commit | Ctrl+↓ / Ctrl+↑ |
| Newest / oldest commit | Home / End |
| Go to the checked-out commit | Ctrl+H |
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

**Settings** (the gear, or Ctrl+,) is in sections down the side; type in **Search settings** (Ctrl+F while it's open) to find any setting by name or by what it's about. Changes apply as you make them, and **Reset to defaults** puts a section back as it started.

- **Appearance:** light, dark or **Match system** (following your computer); the look (Raccoon, the default, Classic, Pastel, Forest, Paper or High contrast); the **interface size**, from 90% to 150% (anywhere in the app, Ctrl+scroll or Ctrl+plus and Ctrl+minus zoom from 50% to 200%, and Ctrl+0 goes back to normal); how the graph's lines look **where a branch starts** and **where a branch is merged**, each on its own (**Curved**, or **Rounded**: along the commit's row, turning into the other lane with a rounded corner), and how round the bends are (**Corner roundness**: at 0, Rounded corners are square and Curved lines go straight across); whether the sidebar shows, and which of its sections; and putting the graph's columns back in their usual order and widths.
- **Your themes:** under the looks, **New theme** makes one from the look in use and opens the editor: a name, which version you're editing (dark or light), and each colour (surfaces, text and accent, changes, and the graph's lines) by its picker or by typing `#rrggbb`. While a theme is in use, the app changes as you edit it; the preview shows the version you're editing. If text would be hard to read (below 4.5:1 contrast), it says so, and **Make it readable** lightens or darkens it just enough. **Duplicate**, **Export…** (a small `.json` file to share) and **Delete theme…** are at the bottom; **Import…** next to Your themes adds one from a file. Click a theme to use it, or a built-in look to go back.
- **Git:** what Pull does when both sides have new commits (ask each time, merge, or rebase your commits on top) and with uncommitted changes; whether Fetch removes remote branches deleted on the remote, and lets fetched branches and tags replace local ones; how often to fetch in the background; diffs unified or side by side, and whether they show the whole file; changed files by path or in folders.
- **Accounts:** the GitHub and GitLab accounts you're signed in to, and adding one.
- **Tools:** the terminal "Open a terminal here" runs (if not the system's own; `{path}` stands for the repository's folder, as in `alacritty --working-directory "{path}"`), and the folder new clones go in.
- **Updates and privacy:** whether to check for updates, and clearing the list of recent repositories.
- **About:** the version and what's new in it, sending feedback, and the licenses.

## Updates and crash reports

**Updates.** An installed Git Raccoon checks for a new version when it starts and every half hour while it's open, and downloads it in the background. When it's ready, the toolbar shows **Restart to update**; your tabs and settings are kept.

**What's new.** After an update, Git Raccoon shows what changed since the version you had (and "What's new in this version" in the command palette shows it again).

**Feedback.** The speech-bubble button in the toolbar (or "Send feedback" in the command palette) takes a bug report, feedback or an idea: write a title and a few details and press **Send**: it's posted as a public issue on Git Raccoon's GitHub page, for the project's board. No account is needed, and nothing else about you is sent; bug reports include the version and system. Tick **Include a screenshot of the window** to send what the window looked like when you opened the form; it's shown to you first, as it's public too.

**Crash reports.** If Git Raccoon runs into a bug, it writes a report on your computer — nothing is sent. Next time you start it, it shows you the report and lets you choose: **Report on GitHub…** opens an issue you can read and add to before posting, **Copy report**, or **Don't send**.

## Troubleshooting

- **"Git isn't installed, or isn't on PATH."** Install [Git](https://git-scm.com), then restart Git Raccoon.
- **Fetching or pushing asks for a password every time.** Set up a credential helper (Git Credential Manager comes with Git for Windows), or use SSH with your key in an SSH agent.
- **"Password authentication is not supported" (GitHub).** GitHub doesn't take your account password for Git. Sign in to GitHub in Git Raccoon (**Settings → Accounts**), or give a personal access token as the password. If an old password is saved, remove it from your credential helper (on Windows: Credential Manager, `git:https://github.com`).
- **"Not owned by current user" / "dubious ownership".** The folder belongs to another user, and Git doesn't trust it. Git's message shows the `git config --global --add safe.directory …` command that trusts it.
- **"This host isn't known yet."** Git Raccoon asks you to confirm a new SSH host the first time; check its fingerprint against the one the host publishes.
- **Large files show as small text files.** The repository uses Git LFS: install [git-lfs](https://git-lfs.com) and run `git lfs pull`.
- **Something went wrong.** The error banner shows Git's own message. If it looks like a bug, a report on the project's [issues page](https://github.com/Trashpanda-GameDev/Git-Raccoon/issues) helps.
