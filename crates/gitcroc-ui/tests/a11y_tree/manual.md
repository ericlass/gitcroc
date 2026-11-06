# Screen reader scripts (M6-25c)

What `just a11y` cannot check: what a screen reader actually says. These
scripts are M6-04's NVDA and VoiceOver scripts, rewritten for
`gitcroc`, plus an Orca variant for Linux. They are run before a release
and each table is filled in (README decisions table, "Accessibility bar in
M6", must 9). At M6-31's sign-off only the Orca script runs: NVDA and
VoiceOver wait, with the other by-hand checks on Windows and macOS, until a
Windows PC and a Mac are available (Eric, 2026-11-06).

## Setup, on every platform

From the repository root, once:

```sh
just bench-data target-project
cargo build --release -p gitcroc-ui --bin gitcroc
```

Then make two small repos from the fixture in a folder of their own, so the
bench data stays untouched. `api` gets an uncommitted file; `web`'s origin
is gone, so its fetch fails:

```sh
mkdir -p ../a11y-walk
git clone --quiet target/bench-data/target-project/repos/repo-02 ../a11y-walk/api
git clone --quiet target/bench-data/target-project/repos/repo-03 ../a11y-walk/web
git -C ../a11y-walk/web remote set-url origin ../a11y-walk/gone.git
echo "for the walkthrough" > ../a11y-walk/api/notes.txt
```

(In PowerShell the same commands work, with `New-Item -ItemType Directory
..\a11y-walk` for `mkdir -p` and `Set-Content` for `echo … >`.)

Start `target/release/gitcroc`. It shares its settings and workspaces
with an installed gitcroc, so make a workspace for the walkthrough: Open
workspace, New workspace…, named `a11y-walk`. Add repository, Add folder…,
and pick `a11y-walk`. Delete the workspace and the folder afterwards.

## The steps

1. **The window opens.** Quit and start the app again. Record what is
   read first and where the focus is.
2. **The history.** Click `api` in the tree, then F6 to the history. Down,
   Down, Up. Record each row read, and whether its position ("2 of 40")
   and "selected" are read.
3. **The panes.** Tab twelve times, then Shift+Tab three times. Record the
   order and what each stop reads. The window's own Minimize, Maximize and
   Close must not be stops; reach them as the platform's notes say, and
   record whether they are read with their names.
4. **The tree.** F6 to the tree. Down, Right, Left. Record whether expanded
   or collapsed and the level are read.
5. **A dialog.** Ctrl+K (Cmd+K on macOS) opens the command palette. Record
   what is read as it opens. Tab three times, then Escape. Record where the
   focus ends: where it was before Ctrl+K.
6. **The commit message.** Select `api`'s uncommitted changes, F6 to the
   changes, Tab to Subject. Type `ab`, then Left twice: each should read a
   character. Read the line. Tab to Description, type `one`, Enter, `two`,
   then Up and Down: each should read a line. Record whether the typed
   text is read back by character and by line.
7. **A failing job.** Click `web` in the tree, then Fetch in the toolbar.
   Without moving the focus, record whether "Fetch … failed" is read once,
   and whether any progress was read.

## NVDA (Windows)

Windows 10 or 11, NVDA 2024.1 or later. Open NVDA's Speech Viewer (NVDA
menu, Tools, Speech Viewer) to copy what it says. Reach the window buttons
with object navigation (NVDA+Shift+arrows, or NVDA+Numpad). In step 6,
NVDA+Up reads the line; Left and Right read characters.

| Step | What NVDA read | Keyboard works? | Notes |
| --- | --- | --- | --- |
| 1 Window opens | | | |
| 2 History rows, position, selected | | | |
| 3 Tab order, window buttons | | | |
| 4 Tree | | | |
| 5 Dialog: opens, Tab, Escape, focus back | | | |
| 6 Commit message by character and line | | | |
| 7 Failure announced once | | | |

## VoiceOver (macOS)

macOS 14 or later. Turn VoiceOver on with Cmd+F5; its caption panel shows
what it says. VO is Control+Option. If Tab only moves between text fields,
turn on Keyboard navigation in System Settings, Keyboard. Reach the window
buttons with VO+Right through the title bar. In step 4, VO+Shift+Down
interacts with the tree first. In step 6, VO+L reads the line.

| Step | What VoiceOver read | Keyboard works? | Notes |
| --- | --- | --- | --- |
| 1 Window opens | | | |
| 2 History rows, position, selected | | | |
| 3 Tab order, window buttons | | | |
| 4 Tree | | | |
| 5 Dialog: opens, Tab, Escape, focus back | | | |
| 6 Commit message by character and line | | | |
| 7 Failure announced once | | | |

## Orca (Linux, X11 and Wayland)

Orca 46 or later, started with Super+Alt+S or `orca` in a terminal, on the
X11 session and on a Wayland one (GNOME, for instance). Reach the window
buttons with flat review (KP_7 to KP_9). In step 6, KP_8 reads the line.
On Wayland, also record whether the window
gets the focus and whether Tab works.

| Step | X11: what Orca read | Wayland: what Orca read | Notes |
| --- | --- | --- | --- |
| 1 Window opens | | | |
| 2 History rows, position, selected | | | |
| 3 Tab order, window buttons | | | |
| 4 Tree | | | |
| 5 Dialog: opens, Tab, Escape, focus back | | | |
| 6 Commit message by character and line | | | |
| 7 Failure announced once | | | |
