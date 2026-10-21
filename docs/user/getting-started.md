# Getting started

## First start

When gitcroc starts, it looks for git. If it finds none, or only one older than 2.40, it shows **gitcroc needs git** with what it found and how to install a current git for your system. Install it and click **Check again**, or click **Choose git…** to pick a git program yourself. [Troubleshooting](troubleshooting.md#git-not-found-or-too-old) has more on this.

With git found, gitcroc opens with no workspace: "No workspace is open. Open one or create a new one." A workspace is a named set of repositories, usually one per project. Click **New workspace…**, type a name such as `Platform`, and click **Create**. The workspace opens in a tab.

## The window

From top to bottom, the window has:

- **The tab bar.** One tab per open workspace. The **+** button at its end (**Open workspace**) lists your workspaces and offers **New workspace…** and **Import workspace…**. See [Workspaces and groups](workspaces-and-groups.md).
- **The toolbar.** **Fetch**, **Pull** and **Push**, each with an arrow beside it for its options; **Branch**, to create a branch; **Merge**, to merge or rebase; **Stash**; then **Command palette** and **Settings**. The buttons act on what is selected in the tree. A button that cannot act on the selection is greyed out, and its tooltip says why.
- **Three panes**, each deciding what the next one shows:
  - **Tree** (left): your groups and repositories, and inside each repository its branches, remotes, tags and stashes.
  - **History** (middle): the commits of what is selected in the tree.
  - **Changes** (right): what changed in the commit selected in the history, or your uncommitted changes.
- **The status bar.** Running jobs with their progress, "No jobs running" when there are none, and a notice when an update is available. Click it to open the job list (**Jobs**).

Drag the borders between the panes to resize them. **Dock at bottom** and **Dock right**, in the changes pane's header, move the changes pane below the other two or back to the right; each workspace remembers its choice.

### The tree

The tree lists your groups first, in your order, then **Ungrouped**, which holds the repositories that are in no group. A group holds a **Branches** node and its repositories. A repository holds **Local** (its branches), **Remotes**, **Tags** and **Stashes**. Branch names with a `/`, such as `feature/login`, fold into folders.

A repository's row shows its name, its current branch, how many commits it is ahead of and behind its upstream, a dot when it has uncommitted changes, a conflict icon while a merge or rebase has stopped on conflicts, a spinner while a job runs on it, and a warning when its folder is missing.

Type in **Filter repos and branches** above the tree to show only the repositories and branches whose names contain the text.

Right-click a row, or select it and press Shift+F10 or the context menu key, for the operations that row offers. Double-click a local branch, or press Enter on it, to check it out.

### The history

With a repository selected, the history shows the commit graph of all its local and remote branches, newest first, with the columns **Graph**, **Subject** (with labels for branches and tags), **Author**, **Date** and **Hash**. With a branch, tag or remote branch selected, it shows only the commits reachable from it; with a stash, the stash itself.

When the repository has uncommitted changes, the first row is **Uncommitted changes (n)**, where n is the number of changed files.

Type in **Search subject, author or hash** to show only the commits that match.

### The changes pane

For a commit, the changes pane shows its header (hash, author, date, the full message and its parents), the list of changed files, and the diff of the selected file. For a merge commit, the diff is against the first parent, and **Compare with** switches to another parent.

In the pane's header, **Inline** and **Side by side** switch the diff mode, and beside them are **Ignore whitespace**, **Wrap lines** and **Context** (3 lines, 10 lines or the whole file). These choices are remembered for all workspaces. Above the file list, **Flat** and **Tree** switch between a plain list and a folder tree.

Binary files show their size before and after; images show both versions. A very large diff stays folded until you click **Load diff**.

## Adding repositories

Click **Add repository** at the top of the tree:

- **Add folder…** picks a folder. If it is a repository, it is added. If not, gitcroc searches the folder and its subfolders, four levels deep, and adds every repository it finds. **Add result** then lists what was added, what was in the workspace already, and anything refused, with the reason.
- **Clone…** opens **Clone a repository**: the **URL**, the parent folder (**Into folder**, picked with **Choose…**), and the **Folder name**, which gitcroc suggests from the URL. **Add to group** can put the clone straight into a group, or into a **New group…** you name there. Click **Clone**; the clone runs as a job, and the repository is added when it finishes.

A repository is in a workspace only once, however often you add it. **Remove from workspace**, in a repository's menu, takes it out of the workspace and leaves the folder on disk as it is.

## Your first commit

1. Select the repository in the tree. The history's first row reads **Uncommitted changes (n)**; select it.
2. The changes pane shows two lists, **Staged** and **Unstaged**. New files that git does not track yet are under **Unstaged**.
3. Select a file to see its diff. Click the **+** button on its row (**Stage**), or press Space, to stage the whole file, or **Stage hunk** on one block of changes to stage only that block. **Stage all** stages every file. To take something back, use the **−** button (**Unstage**), **Unstage hunk** or **Unstage all**.
4. Type the **Subject**, the first line of the message, and if you like a **Body** below it.
5. Click **Commit**, or press Ctrl+Enter (Cmd+Enter on macOS) in the message box.

The commit appears at the top of the history. To change it, stage more changes or edit the message, then click **Amend**; with an empty message box, Amend first fills in the last commit's message.

Commit hooks, sign-off and signing follow your git config, as they do on the command line. If git refuses the commit, for example because a hook failed, gitcroc says so, and the job list shows what git said.

To publish your commit, click **Push**. To get your colleagues' commits, click **Fetch** or **Pull**. [Operations](operations.md) describes these and every other operation.
