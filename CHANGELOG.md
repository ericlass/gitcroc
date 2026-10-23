# Changelog

All notable changes to gitcroc are documented in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). [RELEASING.md](RELEASING.md) says how a section becomes a release.

## [Unreleased]

### Added
- **The tab bar is the title bar.** gitcroc draws its own title bar: drag the tab bar's empty space to move the window, and double-click it to maximize or restore the window. On Windows and Linux the minimize, maximize and close buttons sit at its right end; macOS keeps its own window buttons. Settings, **Window**, **Use the system title bar** brings back your system's title bar.
- **Move history columns.** Drag a column's header in the history to move the column, or use **Move left** and **Move right** in the header's menu. **Reset columns** in the same menu restores the default order and widths.

### Fixed
- Resizing the history's columns works: the column edges are easier to grab and show a line, an edge follows the pointer on both sides of the subject, and each workspace keeps its column widths and order after a restart.
- A repository's name in the tree stays whole when the tree is narrow. Its branch name is shortened with an ellipsis instead of hiding the name.
- The changes pane no longer leaves the window when the window gets smaller after the pane was made narrow. It shrinks to its minimum size instead, and gets its size back when the window grows again.

## [1.0.0-rc.1] - 2026-10-21

The first release candidate of gitcroc 1.0: a Git client for Windows, macOS and Linux, made for projects that span many repositories. The [user guide](https://github.com/ericlass/gitcroc/blob/main/docs/user/README.md) describes everything below.

### Added
- **Workspaces and groups.** A workspace holds a project's repositories, each workspace in a tab of its own. Add a repository, every repository in a folder, or a clone. Gather repositories into groups, and share a workspace as a file.
- **Operations on a group.** Fetch, pull, push, check out, create and delete a branch, merge, rebase, tag and commit in every repository of a group, or of a selection, in one action. A preflight shows what will happen in each repository first, and the result report gives each repository its own line. Each repository keeps its own branch and state.
- **Tree, history and changes.** The tree shows groups, repositories and their branches, remotes, tags and stashes, with ahead and behind counts. The history shows the commit graph of a repository, a branch or a whole group. The changes pane shows a commit's diff inline or side by side, with images and binary files.
- **Working copy.** Stage and unstage files and hunks, discard changes, commit and amend. Hooks, sign-off and signing follow your git config.
- **Everyday operations.** Branches, fetch, pull and push with their options, merge and rebase, cherry-pick and revert, stash, tags, reset and remotes. Every change is made by running git, so your hooks, config, credential helpers and LFS filters apply.
- **Conflicts.** When a merge, rebase, cherry-pick, revert or stash pop stops on conflicts, the conflict editor shows each conflict with the branch each side comes from. Take ours, theirs or both, or edit the text, then **Mark resolved** and **Continue** or **Abort**.
- **AI help with conflicts.** Optional and off until you set up a provider: your own Anthropic or OpenAI-compatible API key (local models included), or the Claude Code or Codex command-line tool. gitcroc asks before it first sends anything, and shows what it sends. The AI only proposes resolutions; you accept, edit or reject each one, and nothing is written until you do. Keys are kept only in your system's keychain.
- **Keyboard and accessibility.** Every action works from the keyboard, with a command palette (Ctrl+K, Cmd+K on macOS) and shortcuts in every pane. Controls are labelled for screen readers.
- **Look and settings.** Light and dark themes, code themes and fonts. gitcroc finds your git (2.40 or newer), or uses the one you choose.
- **Speed.** Built to stay responsive with a project of about 20 repositories, 200,000 files and 2 million lines of code.
- **Install and updates.** An installer and a portable zip for Windows, a universal disk image for macOS, and an AppImage, a `.deb` and an `.rpm` for Linux. Every package checks for updates once a day, which you can turn off in Settings. The Windows installer, the macOS app and the AppImage then update themselves after you agree.
- **Privacy.** No telemetry and no account. gitcroc connects only to your remotes (through git), to the AI provider you set up, and to GitHub for the update check.

### Known issues
- **Linux `.deb` and `.rpm`:** `apt`, `dpkg`, `dnf` and `rpm` sort `1.0.0-rc.1` after `1.0.0`, so they would call installing 1.0.0 over this release candidate a downgrade. Before installing 1.0.0 from one of these packages, uninstall the release candidate (`sudo apt remove gitcroc` or `sudo dnf remove gitcroc`); your settings and workspaces stay. The AppImage, Windows and macOS are not affected.
