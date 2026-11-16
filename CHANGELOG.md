# What's new in Git Raccoon

Each version's notes appear in the app after updating to it (and on its
release page). Newest first; a version's heading is just its number.

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
