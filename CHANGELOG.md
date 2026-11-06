# Changelog

All notable changes to gitcroc are documented in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). [RELEASING.md](RELEASING.md) says how a section becomes a release.

## [Unreleased]

## [1.0.0] - 2026-11-06

The first release of gitcroc: a Git client for Windows, macOS and Linux, made for projects that span many repositories. gitcroc draws its own native window. The [user guide](https://github.com/ericlass/gitcroc/blob/main/docs/user/README.md) describes everything below.

### Added
- **Workspaces and groups.** A workspace holds a project's repositories, each workspace in a tab of its own. Add a repository, every repository in a folder, or a clone. Gather repositories into groups, and share a workspace as a file.
- **Operations on a group.** Fetch, pull, push, check out, create and delete a branch, merge, rebase, tag and commit in every repository of a group, or of a selection, in one action. A preflight shows what will happen in each repository first, and the result report gives each repository its own line. Each repository keeps its own branch and state.
- **Tree, history and changes.** The tree shows groups, repositories and their branches, remotes, tags and stashes, with ahead and behind counts. The history shows the commit graph of a repository, a branch or a whole group; drag its columns' edges to resize them and their headers to move them, and each workspace keeps its columns. The changes pane shows a commit's diff inline or side by side, with images and binary files.
- **Working copy.** Stage and unstage files and hunks, discard changes, commit and amend. Hooks, sign-off and signing follow your git config.
- **Everyday operations.** Branches, fetch, pull and push with their options, merge and rebase, cherry-pick and revert, stash, tags, reset and remotes. Every change is made by running git, so your hooks, config, credential helpers and LFS filters apply.
- **Conflicts.** When a merge, rebase, cherry-pick, revert or stash pop stops on conflicts, the conflict editor shows each conflict with the branch each side comes from. Take ours, theirs or both, or edit the text, then **Mark resolved** and **Continue** or **Abort**.
- **AI help with conflicts.** Optional and off until you set up a provider: your own Anthropic or OpenAI-compatible API key (local models included), or the Claude Code or Codex command-line tool. gitcroc asks before it first sends anything, and shows what it sends. The AI only proposes resolutions; you accept, edit or reject each one, and nothing is written until you do. Keys are kept only in your system's keychain.
- **Keyboard and accessibility.** Every action works from the keyboard, with a command palette (Ctrl+K, Cmd+K on macOS) and shortcuts in every pane. Controls are labelled for screen readers.
- **The tab bar is the title bar.** Drag the tab bar's empty space to move the window, and double-click it to maximize or restore the window. On Windows and Linux the minimize, maximize and close buttons sit at its right end; macOS keeps its own window buttons. Settings, **Window**, **Use the system title bar** brings back your system's title bar.
- **Look and settings.** Light and dark themes, code themes and fonts, in four Settings tabs: **Appearance**, **Git**, **Updates** and **AI**. gitcroc finds your git (2.40 or newer), or uses the one you choose.
- **Speed.** Built to stay responsive with a project of about 20 repositories, 200,000 files and 2 million lines of code, in about 300 MB of memory.
- **Install and updates.** An installer and a portable zip for Windows, a universal disk image for macOS, and an AppImage, a `.deb` and an `.rpm` for Linux. Nothing else is needed on Windows or macOS; on Linux the window draws with Vulkan or OpenGL and needs a working graphics driver, and the `.deb` and the `.rpm` depend on a system font. Every package checks for updates once a day, which you can turn off in Settings. The Windows installer, the macOS app and the AppImage then update themselves after you agree.
- **Privacy.** No telemetry and no account. gitcroc connects only to your remotes (through git), to the AI provider you set up, and to GitHub for the update check.

### From a release candidate
- 1.0.0 installs over 1.0.0-rc.1 and 1.0.0-rc.2 as an upgrade, and your settings and workspaces stay. 1.0.0-rc.1's web-based interface is replaced by the native window.
- On Linux, `apt`, `dnf` and `rpm` show the `.deb` and the `.rpm` as version `1:1.0.0`, so that they sort after the release candidates. You no longer need to uninstall 1.0.0-rc.1 first.
