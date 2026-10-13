# M2-14 Clone, open in file manager and open in terminal

## Goal
The user clones a repo into a folder with progress and adds it to the workspace, and opens any repo in the system file manager or a terminal.

## Depends on
M2-03, M2-07

## Spec
Respects: D10, ARCH-1, ARCH-3, ARCH-7, GRP-1, GRP-10, PLAT-1, PLAT-2, SEC-2, SEC-3, DATA-3, section 6 "Repos" row

## In scope
- Clone:
  - Dialog: the URL, a parent folder picked with the folder picker of M1-05 (opened from Rust, SEC-2), and the folder name, which defaults to the URL's last path segment without `.git`. A folder that exists and is not empty is refused before git runs.
  - `git clone --progress -- <url> <path>` through the runner with askpass (M2-03). Progress as in M2-07. Cancel kills the process (GRP-10) and removes the folder the clone created, and only that folder.
  - On success the repo is added to the open workspace as M1-05 adds a folder (GRP-1), and selected. Adding it to a group on completion arrives with groups (M3).
  - A failed clone is *failed* with git's message and leaves no folder behind.
- Open in file manager and open in terminal, from the repo's context menu, behind one PLAT-1 trait each in `gitcroc-app`, with their `cfg` blocks only in the implementations (the PLAT-1 test of M2-03 covers them):
  - File manager: `xdg-open` on Linux, `open` on macOS, `explorer` on Windows, with the repo path as an argument (ARCH-3).
  - Terminal: on Linux `x-terminal-emulator`, then `gnome-terminal`, `konsole` and `xterm`, whichever is found first, started in the repo folder; on macOS `open -a Terminal <path>`; on Windows Windows Terminal (`wt -d <path>`) when found, else `cmd` in the folder. A missing terminal is reported as an error naming what was tried.
- Toolbar: none; clone is in the "add repository" menu of the tree (M1-07), beside Add folder.
- Tests:
  - Clone (git integration) from a `file://` bare repo: succeeds, the repo is in the workspace and selected; a non-empty target refused; a bad URL fails and leaves no folder; cancel of a clone from a remote that never answers (as in M2-07) removes the partial folder.
  - Open: the command each platform runs is built from the trait with the path as one argument, tested without starting it.
  - Vitest: the clone dialog's default folder name and its refusal of an existing folder as the core reports it.

## Out of scope
- Import workspace, which offers clones for missing repos (DATA-2; M3).

## Acceptance
- `just dev`: clone a public repo over HTTPS with progress, see it added and selected; open it in the file manager and in a terminal.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
