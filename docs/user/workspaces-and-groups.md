# Workspaces and groups

## Workspaces and tabs

A workspace is a named set of repositories and groups, typically one per project. You can have as many workspaces as you like, and several open at once, each in its own tab: a Java project and a C# project side by side, for example. A workspace is open in at most one tab; opening it again switches to its tab.

The **+** button after the tabs (**Open workspace**) lists every workspace, marking those already **(open)**, and offers:

- **New workspace…**: type a name and click **Create**.
- **Import workspace…**: open a workspace file someone shared with you (see [Sharing a workspace](#sharing-a-workspace)).
- **Delete…** beside a workspace that is not open: deletes its workspace file after you confirm. The repositories on disk are not touched.

Right-click a tab, or press Shift+F10 on it, for **Tab actions**:

- **Rename…** the workspace.
- **Export workspace…** to share it.
- **AI for this workspace**: a switch that turns AI help off for this workspace alone. It shows only while AI features are on (see [Conflicts and AI](conflicts-and-ai.md#turning-ai-off)).
- **Close** the tab. Closing a tab does not delete the workspace; it stays in the **+** list. If a conflict editor in it holds edits you have not marked resolved, gitcroc lists those files first, because closing loses them.

Drag tabs to reorder them. Each tab keeps its own selection, expanded tree nodes, scroll positions, pane sizes and filter text, and switching back restores them exactly.

Jobs keep running in a tab you have switched away from. A tab shows how many of its jobs are running or waiting, and a marker (**a job failed**) when one of its jobs failed or stopped on a conflict and you have not looked at it yet. The open tabs, their order and the active tab come back the next time you start gitcroc.

## Groups

A group is a named list of repositories inside a workspace, such as `Backend` or `Frontend`. A group has no branch or state of its own: it is only a list. A repository can be in no group, in one, or in several. Groups do not contain other groups.

- **Create a group:** right-click **Ungrouped**, a group or a repository and choose **New group…**. Group names are unique within the workspace, ignoring case, and up to 64 characters long.
- **Add a repository to a group:** drag it onto the group, or choose **Add to group** in its menu and pick the group (or **New group…**). Dragging a repository from one group to another moves it; hold Ctrl or Alt while dragging to add it to the second group as well. A repository in several groups appears under each of them; it is the same repository either way.
- **Remove a repository from a group:** **Remove from group** in its menu, under that group. It stays in the workspace.
- **Reorder:** **Move up** and **Move down** in the menu of a group, or of a repository inside a group.
- **Rename…** or **Delete…** a group from its menu. Deleting a group leaves its repositories in the workspace. Nothing on disk is ever touched by changing groups.

### What a group row shows

A group's row shows its name, how many members it has, its branch, and how many members have uncommitted changes or are in conflict state.

The branch is worked out from the members each time: if every member is on the same branch, the row shows that branch. If they are on different branches, it shows **mixed** with the number of different branches, for example **mixed (2)**; hover over it for "The members are on different branches".

The group's **Branches** node lists every local branch name that exists in at least one member, with how many members have it, such as `feature/x (3/5)`. Double-click a name, or press Enter on it, to check it out in the whole group.

### Each repository stays your own

Being in a group never stops you from working on one repository alone. Select a single repository and anything you do acts on that repository only. For example:

1. Select the group `Backend`, choose **Check out branch…** in its menu and pick `main`. All five repositories switch, and the group shows `main`.
2. Select one repository in the group and check out `feature/x`. Only that repository switches.
3. The group now shows **mixed (2)**; each repository's row shows its own branch.
4. **Pull** on the group still works: each repository pulls the branch it is on.
5. **Check out branch…** `main` on the group again brings everything back in line.

## The target set and multi-selection

What an operation runs on is decided by your selection in the tree, called the target set:

- **one repository**, when a repository (or something inside it, such as a branch) is selected;
- **all members of a group**, when the group is selected;
- **several repositories**, when you select more than one repository row.

To select several repositories, Ctrl-click (Cmd-click on macOS) each one, or click one and Shift-click another to take every repository row between them. With the keyboard, Shift+Up and Shift+Down extend the selection, Ctrl+Up and Ctrl+Down (Cmd on macOS) move without changing it, and Ctrl+Space (Cmd+Space) adds or removes the row. Only repository rows can be selected together; branches, tags and stashes are always selected one at a time.

With a group or several repositories selected, the history shows one list of the commits of every repository, newest first, with a **Repository** column in place of the graph. Above it, a strip has one line per repository with its branch, ahead and behind counts, changed files and conflict state; click a line to select that repository. If any of them has uncommitted changes, the first row is **Uncommitted changes (n repos)**, where you can stage in each repository and commit them all with one message (see [Commit in several repositories](operations.md#commit-in-several-repositories)).

How operations on more than one repository run is described in [Operations on a group](operations.md#operations-on-a-group).

## Sharing a workspace

A workspace can be shared with your team as a file, for example one committed next to your project.

**Export workspace…**, in a tab's menu or the command palette, writes the workspace to a file you choose. Repository paths are written relative to that file, so the file works on any computer where the repositories sit in the same places relative to it. A repository that no relative path reaches, such as one on another drive, keeps its full path, and the report after the export lists it.

**Import workspace…**, in the **+** menu, reads such a file into a new workspace and opens it. Its name is the one in the file, with "(2)" added if you already have a workspace of that name. Repositories found where the file says are added. If any are not on this computer, **Missing repositories** opens and lists them with the remote they can be cloned from. Choose the folder for each, check the ones you want, and click **Clone selected**. You can open the same dialog later from a missing repository's menu with **Clone from remote…**.
