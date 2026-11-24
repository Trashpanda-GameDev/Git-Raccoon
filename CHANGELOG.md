# What's new in Git Raccoon

Each version's notes appear in the app after updating to it (and on its
release page). Newest first; a version's heading is just its number.

## 0.5.31

- **Compare images three ways.** A changed image can be shown **Side by side**, with **Swipe** (one frame, and a line you drag across it: the old version on its left, the new on its right) or as an **Onion skin** (the new version laid over the old, with a slider for how much of each shows). Your choice is kept.
- **Images in pull/merge requests.** A request's Changes tab shows its images (before and after) instead of saying the file is binary, and so does a new request's list of changes.
- **Finding which commit broke something:** *Starting…* shows until the first step, the progress bar is easier to see, and short steps show tenths of a second.

## 0.5.30

- **Find which commit broke something.** Right-click the commit where it's broken (or use the command palette) › *Find which commit broke something…*, and say where it last worked (the latest tag below it is suggested). Git Raccoon checks out commits in between, halving the range each time: answer **It works**, **It's broken** or **Skip this one**, until it's found. It's Git's own bisect, in plain words.
  - **A bar** above the graph says what's being tested and how many commits and steps are left; the graph marks the range still in question and each answer, and *So far* lists them, with *Undo last answer* and *Open a terminal here*.
  - **Or let a command test each commit** (exit 0 works, 125 can't be tested, anything else broken): each step with its result, time and output, the one running live, and Stop. The command is remembered for the repository; a step can be given a time limit.
  - **The answer:** the first broken commit with its message, author, files and the request it came with, how it was found, and *Show it in the history*, *Revert it…*, *Create a branch here…*, *Copy a summary* (for an issue) and, with an agent connected, *Ask an agent why*. When skipped commits leave it uncertain, the ones it could be.
  - **Uncommitted changes** are put aside first and brought back at the end; *Stop* always goes back to where it started.
  - **Not only for breakages:** look for when something changed, in your own words ("fast" and "slow"), used throughout.
  - **A search started in a terminal** (`git bisect`) shows here too, answers given there included, and can be finished here.
  - **Agents** can start a search (asking you first), answer, run a test command on each commit and read where it stands.
- **The profile switcher** lists each profile's accounts one per line, and a profile with several no longer spills into the next.

## 0.5.29

- **Profiles: who you commit as, and with which accounts.** A profile has a name, a colour, the name and email to commit as, and the GitHub and GitLab accounts it uses (one per forge). Switch at the top right, or from the command palette; Git Raccoon's commits use the profile's name and email (your Git config is left alone), and pushing, pulling, pull/merge requests and cloning use its accounts.
  - Your first run of this version makes a **Default** profile from your Git name and email, with your signed-in accounts: nothing changes until you add another.
  - **Settings › Profiles**: add, duplicate and delete profiles, and choose each one's accounts (one account can be in several profiles). *Sign in to another account…* adds one to that profile.
  - **The commit box** says who you're committing as (click to switch), and Commit says "as Work" once there's more than one profile. When the repository is on the space of an account only another profile has, it warns, with *Switch to Personal*.
  - **Settings › Accounts** says which profiles use each account, and signing out says which lose it first.
- **Search a file's history**: with *History* on, the box at the top of the list (or Ctrl+F) finds the commits that changed the file by message, author, SHA, or any file they changed, as you type.
- **Search filters**, in a file's history and in the graph's search: `author:`, `committer:`, `message:`, `sha:`, `path:`, and dates with `after:`, `before:` and `date:` (`2026-11-23`, `2026-11`, `2026`, `yesterday`, or `2w` ago). They combine, with words besides: `loader author:ada after:2026-06`. The funnel button lists them. The search runs through the whole history as you type, without building an index.
- **Copy a commit's SHA**: right-click a commit › *Copy SHA* or *Copy short SHA*.
- **Arrow keys go through the files**: with a file open, ↑ and ↓ open the previous or next one in its list (a commit's files, or the staged or unstaged changes).
- **Reactions**: the emoji people reacted to a pull or merge request, and to its comments, with show under them, with how many ("👍 2"), on GitHub and GitLab; one without an emoji here shows its name. Yours are highlighted: click one to add yours or take it back, or add another with the smiley button. Emoji names in comments (`:tada:`) show as the emoji.
- **Collapsed sidebar sections stay collapsed** after a restart, for each repository.
- **Dragging a branch scrolls the sidebar**: hold it near the top or bottom of a section to reach the branches out of view.

## 0.5.28

- **Set up SSH for GitHub or GitLab, start to finish**: one window takes you from no key to a working push, for your signed-in account: a key (yours, or a new one), remembered by the agent, added to your account, a connection test, and the repository's remote switched to SSH. Steps already done are ticked and skipped, each says what it changes first, and each can be done by hand instead. Open it from Settings › SSH, the clone dialog, a remote's menu, or an SSH error.
- **Settings › SSH**: your SSH keys in one place, each saying whether it's in the agent and on which of your GitHub and GitLab accounts, with *Copy public key*, *Add to the agent* (the passphrase asked in the app), *Add to GitHub/GitLab* and *Show in folder*.
  - *Create a key…* makes an ed25519 key in `~/.ssh` (with a passphrase, or none after a warning) without a terminal, never overwriting one, and can add a `~/.ssh/config` entry for a host, shown first. *Use an existing key…* adds one from elsewhere.
  - The agent: which one, whether it's running, and *Add keys to the agent when they're first used*.
  - The hosts SSH trusts, and *Test connections* to your accounts' hosts ("Signed in to github.com as rocky.", or exactly why not).
  - Signing in to GitHub now also asks to manage SSH keys; an older sign-in is offered a fresh one when it's needed.
- **SSH errors that say what's wrong, with the fix**: when a fetch, pull or push fails over SSH, Git Raccoon tests the connection and checks your keys, the agent and your account, then says what's wrong ("Couldn't push: GitHub didn't accept your SSH key") with the checks it made, and the fix: *Add the key to GitHub and retry*, *Add it to the agent and retry*, *Check the host's key and retry*, *Use port 443* when port 22 is blocked, *Use HTTPS for origin instead*, or *Set up SSH…*. A background fetch that fails this way says so quietly, in Fetch's tip. A repository's own `core.sshCommand` is kept for Git Raccoon's Git in every case.
- **SSH or HTTPS, your choice**: cloning one of your repositories offers *Connect with SSH / HTTPS*, choosing SSH when your key already works with that forge. A remote's menu (right-click it in the sidebar) has *Use SSH instead* / *Use HTTPS instead*, *Use port 443* for GitHub when SSH's usual port is blocked, and *Test connection*; a changed address shows both first, and can be undone.
- **Trusting a new host, safely**: the first time SSH connects to a host, the question says whether its key is GitHub's or GitLab.com's published one ("This is GitHub's published key"), warns in red when it isn't, and otherwise shows the fingerprint to compare. Settings › SSH marks the trusted hosts whose keys match. When a known host's key has changed (SSH refuses), *Test connections* explains what that can mean and offers to replace the old key only when the new one is the published one.
- **SSH on Windows uses the agent that has your keys**: when your keys are loaded in Windows' OpenSSH agent, Git Raccoon's Git uses Windows' own ssh, so they're used (Git for Windows' bundled ssh can't see them, and asks for the passphrase every time). Only Git Raccoon's commands change; your own `core.sshCommand` still wins.

## 0.5.27

- **Push no longer hides under Search** in a narrow window: the middle buttons stay clear of the right-hand ones, which become icons when space runs short (and long repository and branch names are cut short, in full on hover).
- **Where to push a new branch, more compact**: the name field, *Push* and *Cancel* sit together instead of spread across the window.

## 0.5.26

- **Conflicts as panes**: switch the conflict editor to panes (the two-column button in its header) for a focused view that takes the whole window: each side's whole file next to each other, with its commit, and the file as it will be saved below. Tick a whole conflict with the box at its first line, or click single lines; tick a side's header box to take it everywhere. **Save** writes every conflict you chose at once; the rest stay conflicts. Your choice of cards or panes is remembered.
- **Ticked lines go in the order you tick them**: picking lines for a conflict's result (in cards or panes) puts them in the order you chose them, rather than always your side's first.
- **A tidier conflict editor header**: *Whole file*, *Edit*, *Why?* and *Ask an agent* are icon buttons (their names show on hover), so the file's name and the way between conflicts stay in view.

## 0.5.25

- **Conflicts in one place**: a merge, rebase, cherry-pick or revert that stops on conflicts now shows them all in place of the history: what's going on ("Rebasing feature/login onto main · commit 3 of 5: *Remember the last user*"), how far along you are, and every file with what it needs (*Resolve…*, *Keep main's* / *Delete it* for a file deleted on one side). Resolved files can be reviewed, or their conflict brought back. **Skip this commit** for rebases. Sides are named by branch, not "ours" and "theirs".
- **A three-way editor for conflicts**: each conflict shows what both sides started from, next to each side's version (with the commit that changed it), whatever your Git's conflict style. Take one side or both (in either order), or tick the lines you want from any of the three and touch up the result. Ctrl+↑ / Ctrl+↓ move between conflicts.
- **Know before it conflicts**: merging and rebasing say beforehand whether they would conflict, and in which files ("This will conflict in 2 files: …"), or that they go in cleanly, without touching your files. A pull that merges or rebases asks first when, as of your last fetch, it would conflict.
- **Why a conflict happened**: *Why?* in the conflict editor shows the commits on each side that changed the conflict's lines, with their messages and the request they came in with, and marks them in the graph on request. A hint says what happened when it's clear (a rename on one side, lines only added on the other).
- **Agents can propose a conflict's resolution**: connected agents read the conflicts (each side named by branch, with what both started from) and propose a file's resolution with their reasoning. It's shown in the editor; nothing is written until you choose *Use this* (undoable), *Edit it* or *Discard*. *Ask an agent* gives you the request to paste.
- **Remember how you resolved a conflict** (*Settings › Git › Conflicts*, off to start with): when the same conflict comes back, in a long rebase or merging main again, it's resolved the way you did it before and listed as *resolved as before*, to review (or *Redo by hand*) and mark resolved. Git's own `rerere`, for Git Raccoon only.
- **Restart to update from Settings**: when an update is downloaded, *Settings › Updates and privacy* has the *Restart to update* button too, so you don't have to close Settings to find it in the toolbar.

## 0.5.24

- **Feedback works with no repository open**: the button on the start screen did nothing. So do *What's new*, *Check for updates* and zooming.

## 0.5.23

- **Let AI agents work with Git Raccoon** (Settings › Agents, off unless you turn it on): Claude Code, Claude Desktop, Cursor and others connect over MCP through `git-raccoon mcp`, with the setup ready to copy.
  - They read freely: status, history, commits and diffs, branches, and pull/merge requests with their threads.
  - They can show you things: a commit, a request, a new request filled in for you to open.
  - Changes ask you first (*Allow*, *Allow for this session*, *Deny*); discarding asks every time. Comments they write are drafts you send with your review.
  - A robot in the toolbar shows while one is connected.

## 0.5.22

- **Add a submodule from the sidebar**: *Submodules* is always there now (hide it like any section, by right-clicking a section's heading), and its **+** adds one from a URL, into the folder you choose. It's cloned and staged with `.gitmodules`, ready to commit.
- **Discarding a submodule's changes no longer throws its folder away.** It moved the whole submodule (and any work in it) to the trash; now it puts the submodule back on the commit this repository records, keeping its own commits. If it has work of its own (edited or new files), it's left alone, and you're told to open it to commit or discard those.

## 0.5.21

- **Corner roundness looks like a slider again** (Settings › Appearance): a wider track, filled in the accent colour up to the value, with a round handle. It looked like a button.
- **Pin branches to the top**: right-click a local branch in the sidebar › *Pin to top*. Pinned branches sit at the top of *Local*, in alphabetical order, with a pin; *Unpin* puts one back in its folder. Pins are kept per repository.
- **Move the line between the sides** of a side-by-side diff: drag it to give one side more room; double-click it for half each. It stays where you put it, in every diff.
- **Go from one comment to the next** in a request's *Changes*: the arrows by the file's name, or **Alt+↑** / **Alt+↓**, go to the previous or next thread, across files, and say which one you're on ("2 of 5"). *Show in Changes* on a thread in the conversation goes to it the same way.
- **The arrow keys no longer move the hidden graph** while a request is open.
- **A request's side-by-side diff scrolls like any other**: each side's code scrolls sideways under its own scrollbar, the two together, with the line numbers staying put. It scrolled the whole view, numbers and all.
- **A request's file list keeps its width**: long paths are cut short with "…" (the whole path on hover) instead of widening the list.
- Going to a comment scrolls only up or down, not sideways.
- **Hovering a line in a request's diff no longer pushes the lines below it down**: its + (to comment) now sits over the line instead of taking room.
- **The status under an open request fits the sidebar**: when the words don't fit, it shows only the icons and the thread count, with the words on hover. They wrapped to the panel's edge.
- **Select and copy Markdown text**: drag over a description or comment (in a request or an issue) to select it, then copy it with Ctrl+C.
- **An issue's description shows as Markdown** in the side panel, as a request's does, with its images. It showed the raw text.
- **Markdown looks right** in requests, issues and comments:
  - Links line up with the text and show in the link colour. They sat higher than the words around them.
  - **Bold** is bold. It looked like the rest of the text.
  - Headings come in clear steps, with a line under the top two levels.
  - List bullets and numbers line up with their items, and task lists show a checkbox without a bullet.
  - Code blocks have room around them and syntax colours, like diffs.
  - Tables have a bold header row and more space between cells.

## 0.5.20

- **A request opens even when the forge fails on part of it.** A self-managed GitLab answering an error (a 500) for, say, the approvals or the diffs used to stop the whole view; now the rest shows, and a note says what's missing and why, with *Try again*. A server error is retried once by itself, errors say which call failed, and diffs fall back to GitLab's older way of listing them.

## 0.5.19

- **Long pull/merge requests are complete**: their files, commits, comments and threads were cut off at the first 100; now every page is loaded (up to 5,000 of each, with a note if there are more). The reviewer, assignee and label pickers list everyone and every label too.
- **Load more requests or issues**: the sidebar lists the newest 50 of each, and a *Load more* row at the end lists the next 50, when there are more.
- **Pull the latest works for requests from forks**: their branch (`pr/12`, `mr/12`) now follows the request, so Pull brings its new commits.
- **Images in requests' descriptions and comments** are shown, not just their alt text, including uploads in private repositories (fetched with your account).
- **Your review drafts are kept on GitHub or GitLab too**, as you write them: in GitHub's pending review, or as GitLab draft notes. They're there on the website and on other computers, and drafts started there show up here. (GitHub can't hold comments on the whole request in a review; those stay here until you finish.)
- **Edit or delete your comments** after they're posted (the pencil and bin by them).
- **A new request is a view of its own**, in place of the graph, like an open one: its Description, Commits and Changes in tabs.
- **Bring a request up to date** from the Merge window when it's behind: *Update branch* on GitHub, *Rebase onto…* on GitLab. GitLab projects that only fast-forward get **Fast-forward** as their way of merging.
- **The open request's status in the sidebar**: its checks, approval and open threads, under it.
- **Outdated threads** (on code that's changed since) are listed above a file's diff in Changes.
- Clicking a commit in a request's **Blame** shows it, even when it isn't in your graph.
- **Checks** are read in full too, not just the first 100.

## 0.5.18

- **Pull and merge requests, without leaving the app**: click one in the sidebar and it opens in place of the graph, with how its checks, approvals and threads stand, and **Conversation**, **Commits** and **Changes** tabs. Its description and comments show as formatted Markdown. Changes shows each file's diff with the comment threads under their lines: reply, resolve or unresolve them there.
- **Reviews**: comment on a line (its **+**), on a range of lines (pick them, then right-click › *Comment on selection*), reply to threads, or comment on the whole request. They're drafts, kept until you **Finish review**, then sent together with a summary and *Comment*, *Approve* or *Request changes*.
- **Suggested changes**: a suggestion in a comment shows as the lines it changes and what they'd become. On GitLab, **Apply** commits it to the request's branch, or pick several (*Apply with others*) and apply them in one commit with your own message. On GitHub, whose API can't apply them, **Apply here** changes the file on the request's branch checked out here, for you to commit. To suggest one yourself, pick lines in Changes and right-click › *Suggest a change*: the comment starts with those lines, ready to edit.
- **Merge from the app**: merge commit, squash or rebase (what the repository allows), the message, and deleting the branch after; when it can't be merged yet it says why.
- **Split, Whole file and Blame** in a request's Changes, as in any diff: Git Raccoon fetches its commits in the background (without making a branch for them).
- **A request's commits, marked in the graph**: *Show them in the graph* on its Commits tab, with a bar to go back.
- **Check out ▾** a request's branch, or a new worktree for it that opens in a tab. With uncommitted changes in the way, it asks: put them aside and check out, use a worktree instead, or cancel. Once on it, **Pull the latest**, or go **Back to** the branch you were on, with your changes.
- **Opening a request** shows the commits it will have and the files it changes (with their diffs), warns when it won't merge cleanly or is behind, fills in the title and description from its commits (or the repository's template), lets you pick reviewers, assignees and labels from the project's own, and pushes and opens in one step (even a branch that isn't checked out). **Open a pull/merge request…** is in every local branch's right-click menu too, and pushing a new branch offers to open one.
- **Check for updates now**: a button in Settings › Updates and privacy (and *Check for updates* in the command palette) looks for a new version right away, instead of waiting for the next half-hourly check, and says how it went: up to date, downloaded and ready to restart into, or what went wrong.

## 0.5.17

- **Feedback is posted as you, or by the feedback bot**: signed in to GitHub (Settings › Accounts), the feedback form posts the issue with your own account, so you're its author and see replies. Otherwise Git Raccoon's feedback bot posts it without your name, and you can say how to reach you. It used to be posted under the project's own account.

## 0.5.16

- **A long commit description scrolls** inside its box instead of growing over the file lists, and **the commit box can be resized**: drag its top edge (remembered).
- **Unstaged and staged files share the pane** like the sidebar's sections: half each to start with, whatever is in them, so *Staged files* no longer jumps up under a short unstaged list. Drag the line between them to share the space differently (double-click it for half each); a folded list gives its room to the other. The split is remembered.
- **Shift+click picks a range of branches** in the sidebar: every branch shown from the last one clicked to this one (Ctrl+Shift adds the range to those already picked), as well as Ctrl+click for one at a time.
- **Delete a branch or tag here, on its remotes, or both, in one go**: *Delete…* (on a branch or tag, in the sidebar or on the graph) asks with a checkbox for each place: *Local*, and each remote (for a branch, the remotes that have it). The one you right-clicked starts ticked. Deleting a branch here and on a remote first checks its commits are on some other branch, and asks again if not. Undo puts it back wherever it went, on the remotes too. A branch label on the graph that stands for its remote copy as well (`main ☁`) can now delete that copy too.
- **A hand holds what you drag**: dragging a branch (in the sidebar or by its label on the graph), a tab, or a commit in the interactive rebase planner shows a closed hand, as dragging a column heading already did, instead of the four-arrows pointer Windows shows. Commits in the planner show an open hand when you point at them.

## 0.5.15

- **Fetches in the background**, every 10 minutes to start with: new commits on the remotes show up without pressing Fetch. Settings › Git › *Fetch automatically* sets how often (1 to 60 minutes), or 0 to turn it off. It never asks for a password: a remote that needs one is left for you to fetch, and the Fetch button's tip says when it last fetched (or why it couldn't).
- **Pick the branch a new pull/merge request goes into from a list** of the remote's branches (the default one first), instead of typing its name.
- **Pick lines in a diff to copy or stage**: click a line to pick it, Shift+click (or drag) for a range, Ctrl+click to add or take out one. Right-click them to **Copy** their text (or press Ctrl+C), and in your uncommitted changes to **Stage selection** (or Unstage selection), which takes just the changed ones among them, or to pick every line. Esc lets go of them.
- **History and Blame are toggles beside the diff**, no longer a view of their own: **History** lists every commit that changed the file down the side of its diff (click one to see its change), and **Blame** shows who last changed each line, in a column beside it (best with **Whole file**). Use either, both or neither; they're remembered. Click a commit in the blame to go to it.
- **See the whole file in a diff**: **Whole file** (next to Unified and Split) shows every line of the file with the changes marked in place, instead of just the changes and a few lines around them. It opens at the first change, and is remembered (also in Settings › Git › Diffs and the command palette). Staging hunks and lines works the same.
- **Sidebar sections fit together**: the open sections always fill the sidebar between them, and a clear line between two sections drags just that boundary (if the neighbour is as small as it goes, the next one gives way), so no section gets stuck, and the sections further down stay where they are. A collapsed section keeps its place, its heading right on top of the next section, and its space goes to the others; open it again and it gets its share back. Double-click a line to share the space by what's in each again. Sizes are remembered.
- **Hide sidebar sections you don't use**: right-click any section heading to turn sections on or off (pull or merge requests, issues, tags…), or *Show all sections* to bring them back. The same switches are in Settings › Appearance › Sidebar.
- **Copy a branch's or tag's name** from its right-click menu, in the sidebar or on the graph (a remote branch copies as `origin/feature`).
- **Easier-to-read buttons**: the text on orange buttons in the Raccoon look (and on any look's accent-coloured buttons, your own themes too) now always reads at 4.5:1 or better: the button's colour is deepened just enough. Links and highlights keep the brighter colour, which stands out better on the background.

## 0.5.14

- **Make your own themes**: under Settings › Appearance, **New theme** starts one from the look in use, and the editor sets each colour (surfaces, text, accent, changes and the graph's lines) for its dark and light versions, by picker or by typing `#rrggbb`. The app changes as you edit, a preview shows the other version, and text that would be hard to read is pointed out with a fix in one click. Themes can be duplicated, exported to a file and imported.
- **Choose how the graph's lines look** where a branch starts and where it's merged, each on its own (Settings › Appearance): **Curved**, as before, or **Rounded**, where the line runs along the commit's row and turns into the other lane with a rounded corner (a merge's line leaves the merge commit sideways; a branch's line comes into the commit it started from sideways). **Corner roundness** sets how round the bends are, for both: at 0, Rounded corners are square and Curved lines go straight across.

## 0.5.13

- **A clearer Settings window**: its settings are in sections down the side (Appearance, Git, Accounts, Tools, Updates and privacy, About), each with what it does beside it, and a search box finds any of them. Pull's and Fetch's usual ways, the clone folder, What's new and feedback are there now too, and each section can be put back to its defaults.

## 0.5.12

- **The clone and new repository dialogs start with what needs no account**: a URL to clone from, or a repository only on this computer. Choose one of your accounts at the top to add what it can do: a list of your repositories there to clone from, or making the new one there (with a description, public or private, and cloned or not). The places down the side are gone.
- **Unticked checkboxes in these dialogs show their box** clearly, as the options in the Fetch and Pull menus now do.

## 0.5.11

- **A graph with many branches scrolls sideways**: use the horizontal wheel (or Shift and the wheel) over it, or drag the bar under the Graph heading. Jumping to a commit brings its line into view.
- **Using one side for a whole conflicted file asks first**, as it replaces anything picked or edited in it.
- **Pull options**: the arrow beside Pull pulls from another remote (its branch of the same name, when there are several), and sets how Pull joins both sides' new commits (fast-forward only, which asks when both have moved on; fast-forward if possible, else merge; or rebase) and what it does with uncommitted changes (put aside and back, leave them, or discard them, which asks first). Both are remembered.
- **Set upstream**: right-click a branch and choose *Set upstream…* to pick the remote branch it pulls from and pushes to, or none.
- **Fetch options**: the arrow beside Fetch fetches from one remote (when there are several), and sets whether Fetch prunes remote branches deleted there and whether it forces (both remembered). A forced fetch asks first.
- **The clone dialog has where from down the side** (a URL, GitHub, GitLab.com or a self-managed GitLab, with your repositories there in a list you can search), and clones can be **shallow** (the latest commit only) or **sparse** (only the folders you name).
- **A new repository dialog**: choose where it goes down the side (only on this computer, GitHub, GitLab.com or a self-managed GitLab), and fill in the rest: on a forge, the account, a description, public or private, and whether to clone it; and what it starts with: its first branch's name, a .gitignore and a licence from the forge's templates, and Git LFS. A first commit made on a forge is pushed there. Making repositories on an account has moved here from the clone dialog, which is now just for cloning.
- **The command palette (Ctrl+K) scrolls freely**: its list no longer jumps back to the chosen command when scrolled with the wheel or its bar; it follows the arrow keys as before.
- **Side-by-side diffs scroll sideways** for long lines: each half has a scrollbar along the bottom, as the unified view has, and the two halves move together, by either bar or the horizontal wheel (or Shift and the wheel). The line numbers stay put. They used to cut long lines off.
- **Diffs, file history and blame fill the window**: the lines went only part of the way down, and the rest was left to scroll into.
- **Right-click a branch or tag on the graph** for its menu: what the sidebar offers for it (check out, merge, rebase, rename, delete…), then what can be done with its commit. Before, nothing opened.
- **Move through the graph faster**: Shift+↓/↑ follows the selected commit's branch, past commits on other branches, Ctrl+↓/↑ jumps between commits with a branch or tag, Home and End go to the newest and oldest commit, and Ctrl+H to the checked-out one.
- **Search from the toolbar, in a small box over the graph**: the new Search button does what Ctrl+F does, and the search now opens in a box at the top right of the graph instead of a bar across it. It shows which match you're on ("3 of 40").
- **Resize the graph's columns**: drag the edge after a heading, or double-click it to fit the column to what's on screen. No column gets narrower than its heading, and right-click a heading to put the widths back.
- **Moving a column shows where it will go**: its heading follows the pointer and the columns make room, like the tabs; it goes where the pointer is, however wide it is. Over a heading the pointer is an open hand, and a closed one while it's held.
- **Dragging a tab shows where it will go**: it follows the pointer, see-through, and the other tabs slide aside to make room; let go to put it there.
- **Branch labels no longer stop the graph scrolling**: they only spread out when the pointer rests on them, not while the list scrolls past.
- **Write the commit summary in the WIP row** at the top of the graph: it's the same text as the box under Commit, and the stash dialog starts with it as the stash's title. The field starts small, so the change counts stay in view, and grows as you write.
- **The WIP row's dashed line runs down to the commit you're on**, past any newer commits on other branches.
- **Choose where a new branch is pushed**: pushing a branch that isn't on a remote yet asks where it should go, in a bar under the toolbar, filled in with the same name on origin. Pick another remote or name, and it pushes to and pulls from there from then on.
- **Push with the account you signed in with**: when Git asks for a password, a GitHub or GitLab account signed in to in Git Raccoon answers with its token. Your own credential helpers still come first. Signing in to GitHub now also asks for the workflow scope, which pushing changes to GitHub Actions workflows needs.
- **A clearer message when GitHub refuses a password**: it says to sign in here or use a token, and how to remove an old saved password.
- **Repositories in folders owned by the Administrators group open**, as they do with Git itself; one Git refuses still shows Git's message, with the command that trusts the folder.

## 0.5.10

- **Updates are found while the app is open**: it looks every half hour, even when it's sitting in the background, and a newer release replaces one already downloaded. Before, the restart button usually only appeared after reopening the app.
- **Make the sidebar and the details as wide as you like**: drag their edges; they're no longer held to a fixed width (the graph keeps some room).
- **Move the graph's columns** by dragging their headings; right-click a heading to put them back in order.
- **The sidebar no longer scrolls as a whole**: only each section's list scrolls.

## 0.5.9

- **Open a terminal in the repository** from the toolbar, with Ctrl+` or from the command palette (worktrees and submodules too, from their right-click menu). Choose which terminal in Settings.

## 0.5.8

- **A label cut short grows to its whole name** when you hover over it in the graph, the way several labels spread out, instead of a tooltip.

## 0.5.7

- **Clone from your accounts**: pick one of your GitHub or GitLab repositories from a list, or make a new repository there and clone it, in the Clone window.
- **Sidebar sections keep their headings in view**: each scrolls on its own, and you can drag the line under one to make it taller or shorter (double-click it to let it fit again).
- **Long branch names show in full** when you hover over a label that's cut short in the graph.

## 0.5.6

- **Stashes show in the graph**, with a stash icon and a dotted line to the commit they were made on; right-click one to apply, pop or delete it. ("Drop" is now called "Delete".)
- **Pick several branches** in the sidebar with Ctrl+click (Cmd+click on a Mac) and right-click to delete them together, here or on the remote. It asks once; Undo brings them all back.
- **The checked-out branch runs down the left of the graph**, with the other branches beside it.
- **Labels on light colours have dark text**, so they stay readable in every look.
- **Feedback can include a screenshot** of the window, if you tick the box (it's shown first, as it's public with the issue).

## 0.5.5

- **Pull requests, merge requests and issues** from GitHub, GitLab.com and self-managed GitLab, in the sidebar: see a request's reviews and checks, check it out (forks too), open a new one from your branch, and read issues. Sign in under Settings → Accounts with a token (or, on a self-managed server, with the browser through an app registered there), to several at once; tokens stay in your system's keychain.
- **Open on GitHub/GitLab** from the right-click menu of a commit, branch, remote, tag, pull/merge request or issue (or copy its link).
- **Folders that hold just one folder share its row** in the folder view ("game/assets/localisation"), so deep paths take less room.
- **Delete a branch on the remote**: right-click it under Remote in the sidebar (it asks first, and Undo puts it back).
- **Branch, Fetch, Pull and Push sit in the middle** of the toolbar.
- **Esc in the search bar** closes just the search bar, not the commit beside it.
- What's new also shows after updating from 0.5.2 or earlier; long feedback titles arrive whole.

## 0.5.4

- **Feedback is sent straight from the app**: no browser or GitHub account needed.
- **Links open again**: "Report on GitHub" for crash reports, and other links in the app, open your browser.

## 0.5.3

- **Every look comes dark and light.** Pick a look (Raccoon, Classic, Pastel, Forest, Paper or High contrast) and Dark, Light or System in Settings.
- **Zoom** the whole window with Ctrl+scroll, Ctrl+plus and Ctrl+minus; Ctrl+0 goes back to normal.
- **Pick several changed files** with Ctrl+click or Shift+click, or right-click a folder, to stage, unstage, stash, ignore or discard them together. The buttons follow what's picked ("Stage 2 files").
- **Stash just what's staged**, or just the files you picked.
- **Folder view** for changed files, in the working changes and in commits; the unstaged and staged lists scroll on their own and fold away.
- **Discard all changes** from one button, after asking (files go to the Recycle Bin or Trash). Stash now sits beside Commit.
- **Branch labels on the graph:** hover over "+2" to see them all, double-click one to check it out (a remote branch brings your branch up to it), drag one onto another to merge. A branch and its remote copy on the same commit share one label with a cloud.
- **Tabs** can be dragged into another order.
- **A simpler toolbar**, with a Branch button; the search bar appears with Ctrl+F.
- **What's new** (this!) after each update, and **Feedback** in the toolbar for bugs, feedback and ideas.

## 0.5.2

- **The Raccoon look is the default**, and the look of the website and installers.
- **One Windows installer**, which asks where to install.
- Installer artwork, and welcome and license pages in the macOS installer.

## 0.5.1

- A Windows installer that asks where to install Git Raccoon.

## 0.5.0

- The first release: the commit graph, staging (down to single lines), committing, branches and tags, fetch, pull and push, merging and conflicts, interactive rebase, undo, file history and blame, tabs, themes, and updates that install themselves.
