# gitcroc

[![CI](https://github.com/ericlass/gitcroc/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/ericlass/gitcroc/actions/workflows/ci.yml)

gitcroc is a Git client for Windows, macOS and Linux, made for projects that span many repositories. You keep a project's repositories together in a workspace and gather them into groups. Then you pull, switch branches, create branches and more on a whole group in one action, and get a clear result for each repository. Each repository still stays under your control, so one can sit on a different branch from the rest. When a merge or rebase stops with conflicts, gitcroc can ask an AI model from your own account to propose resolutions. You review each one before anything is written.

## Features

- **Groups of repositories.** Run an operation on a whole group, or on a selection, at once. Every repository runs on its own and gets its own line in the result report.
- **Fast with large projects.** Built to stay responsive with a project of about 20 repositories, 200,000 files and 2 million lines of code.
- **The git you already use.** gitcroc does every change by running git, so your hooks, config, credential helpers and LFS filters apply as they do on the command line.
- **AI help with conflicts.** Wherever a conflict can happen, it proposes resolutions using your own Anthropic or OpenAI-compatible API key (local models included), or the Claude Code or Codex command-line tool you have installed. The AI only proposes; you accept, edit or reject each proposal.
- **One app on three systems.** It works the same on Windows, macOS and Linux, with light and dark themes, code themes and full keyboard control.

v1 leaves some things out, such as pull request integration, interactive rebase and blame. The [non-goals](docs/spec/02-scope.md#non-goals-for-v1-later) list them all.

## Download

Download gitcroc from the [latest release](https://github.com/ericlass/gitcroc/releases/latest). In the file names below, `<version>` stands for the version number:

| System | File to take |
| --- | --- |
| Windows 10 and 11, x64 | `gitcroc_<version>_x64-setup.exe` (the installer), or `gitcroc-<version>-windows-x64-portable.zip` to run without installing |
| macOS 12 and newer, Apple silicon or Intel | `gitcroc_<version>_universal.dmg` |
| Linux x86_64 | `gitcroc_<version>_amd64.AppImage`, or the `.deb` (Debian, Ubuntu) or `.rpm` (Fedora, RHEL, openSUSE) package |

The Windows and macOS builds are not code-signed or notarized, so your system warns about them the first time you open one. [Installing gitcroc](docs/user/install.md) shows how to get past that warning and how updates work.

You also need:

- git 2.40 or newer, which gitcroc does not bundle;
- Windows 10 or 11 (x64), macOS 12 or newer (Apple silicon or Intel), or Linux on x86_64 with glibc 2.35 or newer.

## Documentation

The [user guide](docs/user/README.md) covers everything from the first start onward:

- [Installing gitcroc](docs/user/install.md)
- [Getting started](docs/user/getting-started.md)
- [Workspaces and groups](docs/user/workspaces-and-groups.md)
- [Operations](docs/user/operations.md)
- [Conflicts and AI](docs/user/conflicts-and-ai.md)
- [Keyboard](docs/user/keyboard.md)
- [Settings and files](docs/user/settings-and-files.md)
- [Privacy](docs/user/privacy.md)
- [Troubleshooting](docs/user/troubleshooting.md)

## Privacy

gitcroc has no telemetry and no account. It makes only three kinds of network connection:

- git operations to your remotes, made by git itself;
- requests to an AI provider, only after you have set one up and asked for a resolution;
- a check for updates once a day, which you can turn off in **Settings**, section **Updates**.

[Privacy](docs/user/privacy.md) has the details.

## Building and contributing

[CONTRIBUTING.md](CONTRIBUTING.md) explains how to build gitcroc from source, run the checks and find your way around the code. The specification in [`docs/spec/`](docs/spec/) is the source of truth for how gitcroc behaves.

## License

MIT. See [LICENSE](LICENSE).
