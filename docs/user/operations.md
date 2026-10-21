# Operations

Everything gitcroc does to a repository, it does by running git, exactly as you would on the command line: hooks run, your git config applies, and git asks for credentials the way it always does (see [Credentials](troubleshooting.md#credentials-and-password-prompts)).

You start an operation from the toolbar, from the menu of a row in the tree or the history (right-click, or Shift+F10), from a control in the changes pane, or from the command palette (Ctrl+K, Cmd+K on macOS). Each running operation is a job: it shows in the status bar while it runs, and the job list (**Jobs**, opened from the status bar) keeps every job's outcome and git's own output until you close gitcroc. Select a running job there and click **Cancel** to stop it.

After every job, gitcroc reads the repositories it touched again. Changes made outside gitcroc, in a terminal or another tool, show up on their own, and every repository is read again when the gitcroc window gets the focus back.

## Repositories

| Operation | Where |
| --- | --- |
| Add a repository, or every repository in a folder | **Add repository**, **Add folder…** above the tree |
| Clone | **Add repository**, **Clone…** |
| Remove from the workspace (the folder stays on disk) | Repository menu, **Remove from workspace** |
| Open the folder in your file manager | Repository menu, **Open in file manager** |
| Open a terminal in the folder | Repository menu, **Open in terminal** |

**Open in terminal** opens Windows Terminal (or the Command Prompt when it is not installed) on Windows, Terminal on macOS, and on Linux your desktop's default terminal, else GNOME Terminal, Konsole or xterm, whichever is installed.

[Getting started](getting-started.md#adding-repositories) describes adding and cloning.

## Working copy

Select **Uncommitted changes** at the top of the history to see the **Staged** and **Unstaged** lists. gitcroc stages whole files and whole hunks (blocks of changed lines); it does not stage single lines.

- **Stage** and **Unstage** a file with the **+** and **−** buttons on its row, or select it and press Space or Enter. **Stage all** and **Unstage all** do every file.
- **Stage hunk** and **Unstage hunk** sit on each hunk of the diff. With **Ignore whitespace** on, hunks cannot be staged; turn it off first.
- **Discard** a file's changes with the **↺** button on its row, or select it and press Delete (Backspace on macOS); **Discard hunk** discards one hunk. Discarding cannot be undone, so gitcroc first shows **Discard changes in &lt;repository&gt;?** with what will be lost.
- **Commit** and **Amend**: see [Your first commit](getting-started.md#your-first-commit).

## Branches

| Operation | Where |
| --- | --- |
| Check out a local branch | Double-click it or press Enter on it in the tree; branch menu, **Check out** |
| Check out a remote branch | Remote branch menu, **Check out**: creates a local branch that tracks it, or switches to the one that already does |
| Check out a commit or tag without a branch | Commit menu or tag menu, **Check out (detached)** |
| Create a branch | Toolbar **Branch**; repository menu **New branch…**; branch, remote branch or tag menu **New branch from here…**; commit menu **Create branch here…** |
| Rename a branch | Branch menu, **Rename…** |
| Delete a branch | Branch menu, **Delete…** |
| Delete a branch on the remote | Remote branch menu, **Delete on the remote…** |
| Set or remove the upstream | Branch menu, **Set upstream…** and **Unset upstream** |

**New branch in &lt;repository&gt;** asks for the **Name** and the **Start point**: **Current HEAD**, the **Default branch** (the branch `origin/HEAD` points to, else `main` or `master`), or any **Branch, tag or commit** you name. **Check out the new branch** is on unless you turn it off. Branch and tag names are checked against git's rules before anything runs.

**Delete branch &lt;name&gt; in &lt;repository&gt;?** deletes the branch only if it is merged, unless you check **Delete it even if it is not merged**. **Also delete the branch it tracks on the remote** deletes its upstream too. Both cannot be undone.

Every branch row in the tree shows how many commits it is ahead of and behind its upstream, and **gone** when its upstream no longer exists.

## Sync

| Operation | What it does | Options (the arrow beside the toolbar button) |
| --- | --- | --- |
| **Fetch** | Fetches all remotes | **Prune**: **As git config says**, **Remove remote-tracking branches gone from the remote**, or **Keep them** |
| **Pull** | Updates the current branch from its upstream | **Strategy**: **As git config says (fast-forward only when unset)**, **Fast-forward only**, **Merge** or **Rebase** |
| **Push** | Pushes the current branch to its upstream | **Force with lease (replaces the remote branch)** |

A click on **Fetch**, **Pull** or **Push** runs with the defaults; the arrow beside each, or **Fetch with options…**, **Pull with options…** and **Push with options…** in the palette, opens its options first.

When the branch has no upstream, the push dialog says so and offers to push to a **Remote** you choose with **Set upstream**. A force push replaces the remote branch, so its button reads **Force push**.

## Integrate

| Operation | Where |
| --- | --- |
| Merge a branch into the current branch | Toolbar **Merge** |
| Rebase the current branch onto another | Toolbar **Merge**, with **Rebase &lt;branch&gt; onto it instead** |
| Cherry-pick one commit or several | Commit menu, **Cherry-pick** |
| Revert a commit | Commit menu, **Revert** |

**Merge into &lt;branch&gt; in &lt;repository&gt;** lists the branches, remote branches and tags; type to filter them. **Fast-forward** is **As git config says (fast-forward when possible)**, **Fast-forward only** or **Always create a merge commit**. Check **Rebase &lt;branch&gt; onto it instead** to rebase rather than merge. Rebase in gitcroc is a plain rebase; reordering, squashing or editing commits is not offered.

To cherry-pick several commits, select them in the history first: Ctrl-click (Cmd-click on macOS) or Shift-click, or with the keyboard Shift+Up and Shift+Down and Ctrl+Space (Cmd+Space). They must all be from one repository. The menu entry then reads **Cherry-pick n commits**.

Reverting a merge commit asks which side to keep: **Keep the side of parent** 1 or 2.

When any of these stops on conflicts, the repository is in conflict state: a banner above the history names the operation, with **Abort** to go back to where it started and **Continue** once every conflict is resolved. See [Conflicts and AI](conflicts-and-ai.md).

## Stash

| Operation | Where |
| --- | --- |
| Stash the local changes | Toolbar **Stash**: a **Message (optional)** and **Include untracked files** |
| Apply a stash and keep it | Stash menu, **Apply** |
| Apply a stash and drop it | Stash menu, **Pop** |
| Drop a stash | Stash menu, **Drop…** |

Stash works on one repository at a time. When a pop stops on conflicts, git keeps the stash; after you resolve them, gitcroc asks whether to drop it now (**Drop stash…**) or **Keep it**.

## Tags

| Operation | Where |
| --- | --- |
| Create a tag at HEAD | Repository menu, **New tag…** |
| Create a tag at a commit | Commit menu, **Create tag here…** |
| Push a tag | Tag menu, **Push** |
| Delete a tag | Tag menu, **Delete…**, with **Also delete it on the remote** |

A tag is **Annotated, with a message** (the default) or **Lightweight**. **Push the tag after creating it** pushes it right away. With more than one remote, gitcroc asks which one.

## Reset

Commit menu, **Reset current branch to here…** moves the current branch to that commit. The **Mode** is:

- **Soft: keep the index and the working tree**
- **Mixed: reset the index, keep the working tree**
- **Hard: reset the index and the working tree; uncommitted changes are lost**

A hard reset cannot be undone, so its button reads **Reset hard…** and asks once more.

## Remotes

| Operation | Where |
| --- | --- |
| Add a remote | **Remotes** menu, **Add remote…**: **Name**, **URL**, and **Fetch it after adding it** |
| Rename a remote | Remote menu, **Rename…**; its remote-tracking branches and the upstreams on it move to the new name |
| Edit its URL | Remote menu, **Edit URL…**: the **Fetch URL** and the **Push URL** |
| Remove a remote | Remote menu, **Remove…** |

## Operations on a group

These operations run on more than one repository at once, when a group or several repositories are selected (see [The target set](workspaces-and-groups.md#the-target-set-and-multi-selection)):

| Operation | In each repository it | It skips a repository when | Options |
| --- | --- | --- | --- |
| **Fetch** | fetches all remotes | it has no remote | Prune |
| **Pull** | updates the current branch from its upstream | it is on no branch (detached HEAD), or the branch has no upstream | Strategy |
| **Push** | pushes the current branch to its upstream | it is on no branch | **Set upstream to origin/&lt;branch&gt; where a branch has none** (on by default); force with lease |
| **Check out branch…** | switches to the branch; if there is no local branch of that name but exactly one remote has it, creates a local branch that tracks it | the branch does not exist and **Where the branch does not exist** says **Skip the repository** | **Where the branch does not exist**: **Skip the repository**, **Switch to its default branch** or **Create the branch from HEAD** |
| **New branch…** | creates the branch and switches to it | the branch exists already (it is never reset), or the start point does not exist | Start point; switch after creating |
| **Delete branch…** | deletes the branch | the branch does not exist, or is the current branch | **Force: delete it even if it is not merged**; **Also delete it on the remote of its upstream, else on origin** |
| **Merge…** | merges the branch, tag or commit into the current branch | the name does not exist there, or it is on no branch | Fast-forward |
| **Rebase…** | rebases the current branch onto it | the name does not exist there, or it is on no branch | |
| **Create tag…** | tags its own HEAD | the tag exists already | Annotated or lightweight; push the tag |
| **Commit** | commits its staged changes with the shared message | nothing is staged | **Stage all tracked changes first**; **Push after commit** |

Start them from the group's menu in the tree, from the toolbar, or from the command palette. You type or pick a name once, and each repository looks it up on its own: `main` may be a different commit in each, which is normal.

Everything else (stash, amend, rename a branch, cherry-pick, revert, reset, remotes, discard) works on one repository at a time.

### Preflight and confirmation

Before a group operation runs, gitcroc checks every repository without changing anything (**Checking the repos…**) and puts each in one of four classes:

- **Will run**: nothing stands in the way.
- **Needs attention**: it will run, but look first. For example, a merge or rebase is predicted to conflict, or a branch to push has no upstream. A conflict prediction is only a prediction; the run decides.
- **Will skip**: the operation does not apply to it, with the reason (see the table above).
- **Blocked**: it cannot run, with the reason: its folder is missing, it is in conflict state, or another job is changing it. Fetch still runs on a repository in conflict state.

If every repository will run, the operation starts at once. Otherwise the **Group operation** dialog shows the classes with each repository's reason, and you start it with **Run on n repos**. To leave a repository out, change the selection in the tree and start again. **Always show the preflight dialog**, in Settings, shows this dialog every time.

Group operations that cannot be undone always show the dialog, say what will be lost and name the repositories they will change, with a button that says what will happen, such as **Force push in 3 repos**. These are force push, deleting a branch even if it is not merged, and deleting a branch on the remote. (A hard reset and discarding changes, which work in one repository, always ask too.)

### Running

The repositories run independently and in parallel, 8 at a time by default (**Repos at once** in Settings, 1 to 32). One repository failing never stops or undoes the others: there is no automatic rollback.

**Cancel**, on the job in the job list, skips the repositories that have not started yet and stops running fetches and clones. Other repositories already running finish what they started. Cancelled repositories are reported as skipped (cancelled).

### The result report

When every repository is done, the job's **Result report** in the job list has one line per repository with its outcome:

- **Succeeded**
- **Nothing to do**: for example, the branch was already up to date.
- **Skipped**, with the reason.
- **Failed**, with git's message.
- **Conflict**: git stopped in conflict state. This is not a failure; resolve the conflicts in that repository.

Expand a line to read git's output there, and click the repository's name to select it in the tree. **Retry failed** runs the operation again on the repositories that failed.

### What gitcroc never does for you

gitcroc never stashes, commits or discards your changes to make an operation succeed. If git refuses because of local changes, that repository's outcome is **Failed**, with git's message, and your changes stay as they were.

### Commit in several repositories

With a group or several repositories selected, select **Uncommitted changes (n repos)** at the top of the history. The **Staged** and **Unstaged** lists are divided by repository, and you stage in each as usual. Type one message; the button reads **Commit n repos**, counting the repositories with staged changes. gitcroc first lists each repository with its number of staged files, then makes one commit in each, all with the same message.

The commits are independent: a commit refused in one repository, for example by a hook, is **Failed** there while the others still commit. Amend is not offered here; amend in one repository at a time.
