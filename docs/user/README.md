# gitcroc user guide

gitcroc is a Git client for projects that span many repositories. You keep the repositories of a project together in a workspace, gather them into groups, and run pull, checkout, branch, merge, tag, commit and more on a whole group at once. Each repository still stays under your control on its own. When a merge or rebase stops with conflicts, gitcroc can ask an AI model of your choice to propose resolutions, which you review before anything is written.

gitcroc works through the `git` you already have installed. Everything that changes a repository is done by running git, so your hooks, config, credential helpers and LFS filters all apply as they do on the command line.

## What you need

- **git 2.40 or newer.** gitcroc does not bring its own git. It finds the one on your `PATH`, or you can point it at another (see [Troubleshooting](troubleshooting.md#git-not-found-or-too-old)).
- **One of these systems:**
  - Windows 10 or 11, 64-bit (x64)
  - macOS 12 or newer, on Apple silicon or Intel
  - Linux on x86_64 with glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and later, for example)

## Pages

1. [Installing gitcroc](install.md): which file to download, how to get past the warnings for unsigned builds, and how updates arrive.
2. [Getting started](getting-started.md): the window, adding repositories and your first commit.
3. [Workspaces and groups](workspaces-and-groups.md): workspaces and tabs, groups, selecting several repositories, and sharing a workspace with your team.
4. [Operations](operations.md): every git operation gitcroc offers, and how operations on a group run and report.
5. [Conflicts and AI](conflicts-and-ai.md): resolving conflicts, and setting up AI help.
6. [Keyboard](keyboard.md): the command palette and every shortcut.
7. [Settings and files](settings-and-files.md): every setting, and where gitcroc keeps its files.
8. [Privacy](privacy.md): the network connections gitcroc makes, and the ones it never makes.
9. [Troubleshooting](troubleshooting.md): git problems, credentials, the keychain, and what to put in a bug report.
