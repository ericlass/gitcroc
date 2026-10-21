# Keyboard

Every operation in gitcroc can be reached without a mouse: from the command palette, and from its own place in the window with the keys below.

On macOS, Cmd takes the place of Ctrl in the app-wide shortcuts and the commit box, and Option is the Alt key.

## The command palette

Press Ctrl+K (Cmd+K on macOS), or click **Command palette** in the toolbar. Type to search for:

- **Command**: every operation, such as Fetch, Check out branch… or Mark resolved;
- **Repository** and **Group**: selects it in the tree;
- **Branch** and **Remote branch**: selects it in the tree, with its repository's name beside it.

Up and Down move through the matches, Home and End jump to the first and last, Enter runs the highlighted one, and Escape or Ctrl+K again closes the palette. A command shows its shortcut, if it has one.

A command acts on what is selected, just like its toolbar button or menu entry. When it cannot apply right now, it says what is missing instead, such as "Select a branch in the tree" or "Stage changes first", and does nothing.

## App-wide shortcuts

| What it does | Windows and Linux | macOS |
| --- | --- | --- |
| Open or close the command palette | `Ctrl+K` | `Cmd+K` |
| Move the focus to the next region | `F6` | `F6` |
| Move the focus to the previous region | `Shift+F6` | `Shift+F6` |
| Open Settings | `Ctrl+,` | `Cmd+,` |
| Next tab | `Ctrl+Page Down` | `Cmd+Page Down` |
| Previous tab | `Ctrl+Page Up` | `Cmd+Page Up` |
| Move the tab to the right | `Ctrl+Shift+Page Down` | `Cmd+Shift+Page Down` |
| Move the tab to the left | `Ctrl+Shift+Page Up` | `Cmd+Shift+Page Up` |
| Next hunk in the diff or the conflict editor | `F7` | `F7` |
| Previous hunk in the diff or the conflict editor | `Shift+F7` | `Shift+F7` |

The regions F6 moves between are, in order: the tab bar, the toolbar, the tree, the history, the changes pane and the status bar. The focus lands on the region's selected row or its first control. F7 puts the focus on the first button of the next hunk, such as **Stage hunk** or **Take ours**.

While a dialog is open, only the palette's shortcut works; the others wait until the dialog closes.

## Everywhere

- **Tab** and **Shift+Tab** move between controls; **Enter** or **Space** presses a button.
- **Shift+F10** or the **context menu key** opens the menu of the selected row in the tree, the history or the tab bar. In a menu, **Up**, **Down**, **Home** and **End** move, **Enter** chooses, **Escape** closes.
- **Escape** closes a dialog. Tab stays inside the dialog while it is open.
- The borders between the panes can be focused with Tab; the **arrow keys** then resize the panes.

## Tab bar

| Keys | What they do |
| --- | --- |
| Left, Right | Switch to the tab beside |
| Home, End | Switch to the first or last tab |
| Delete | Close the tab |
| Shift+F10, context menu key | Open the tab's menu (Rename…, Export workspace…, AI for this workspace, Close) |

## Tree

| Keys | What they do |
| --- | --- |
| Up, Down | Select the row above or below |
| Home, End | Select the first or last row |
| Right | Expand the row, or go to its first child |
| Left | Collapse the row, or go to its parent |
| Enter | Check out a local branch; check out a name of a group's **Branches** node on the whole group; expand or collapse any other row |
| Shift+Up, Shift+Down | Extend the selection over repository rows |
| Ctrl+Up, Ctrl+Down (Cmd on macOS) | Move without changing the selection |
| Ctrl+Space (Cmd+Space on macOS) | Add the repository row to the selection, or take it out |

## History

| Keys | What they do |
| --- | --- |
| Up, Down, Page Up, Page Down, Home, End | Select another commit |
| Shift with any of those | Extend the commits selected for a cherry-pick |
| Ctrl with any of those (Cmd on macOS) | Move without changing the selection |
| Ctrl+Space (Cmd+Space on macOS) | Add the commit to the selection, or take it out |
| Shift+F10, context menu key | Open the commit menu |

## Changes pane

| Keys | What they do |
| --- | --- |
| Up, Down, Home, End | Select another file |
| Space or Enter | Stage the selected file, or unstage it in **Staged** |
| Delete (Backspace on macOS) | Discard the selected unstaged file's changes, after you confirm |
| F7, Shift+F7 | Next or previous hunk of the diff |
| Ctrl+Enter (Cmd+Enter on macOS) in the message box | Commit |

## Conflict editor

| Keys | What they do |
| --- | --- |
| Alt+Down, Alt+Up | Next or previous conflict |
| Alt+1 | Take ours in the conflict at the cursor |
| Alt+2 | Take theirs |
| Alt+3 | Take both |
| Alt+4 | Accept the AI proposal at the cursor |
| Alt+5 | Reject it |
| Alt+6 | Edit it |
| Ctrl+Enter (Cmd+Enter on macOS) | Mark resolved; while editing a proposal, accept the edited text |
| Escape | Cancel editing a proposal |

On macOS, Alt is the Option key.

## Job list and result report

Click the status bar, or use the palette's **Show jobs**, to open the job list. Up, Down, Home and End move between jobs, and between the lines of a result report. Escape closes the list.
