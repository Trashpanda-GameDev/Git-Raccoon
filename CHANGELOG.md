# What's new in Git Raccoon

Each version's notes appear in the app after updating to it (and on its
release page). Newest first; a version's heading is just its number.

## 0.5.16

- **Delete a tag or branch on the remote too, or only there**: a tag's menu has *Delete…* (here, as before), *Delete on origin…* (kept here, to fix and push again) and *Delete here and on origin…*, one pair per remote. On the graph, where a branch and its copy on the remote share one label (`main ☁`), its menu has the same three, so the remote copy can be deleted from there too. Deleting a branch in both places first checks its commits are on some other branch, and asks again if they aren't. Undo puts everything back, on the remote as well.
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
