# Troubleshooting

## git not found or too old

gitcroc needs git 2.40 or newer. At start it looks for `git` on your `PATH`; on Windows it also looks where Git for Windows installs itself (under `Program Files` and in your user's `AppData\Local\Programs`). When it finds none, or only an older one, it shows **gitcroc needs git**, with where it looked or what it found, and how to install git:

- **Windows:** install Git for Windows from [git-scm.com](https://git-scm.com/), or run `winget install --id Git.Git -e`.
- **macOS:** install a current git with Homebrew: `brew install git`. The git that comes with Apple's command line tools may be older than 2.40.
- **Linux:** install git with your package manager. On older Ubuntu releases, the git-core PPA has a current version.

Then click **Check again**.

If the git you want is somewhere gitcroc does not look, click **Choose git…** and pick the program (`git.exe` on Windows). gitcroc checks its version and remembers it. You can change this later in **Settings**, on the **Git** tab, which shows the git in use with its version and path. **Use automatic detection** goes back to searching by itself. If the git you chose disappears, Settings says "The chosen git was not found" and names the one gitcroc found instead.

## Git refuses a repository: safe.directory and other safety errors

git has safety checks of its own. The best known is for a repository owned by another user, which git refuses with "detected dubious ownership in repository". gitcroc shows git's message as it is, in the job list and on the repository, and never works around such a check.

If you trust the repository, tell git so yourself, as git's message suggests:

```sh
git config --global --add safe.directory /path/to/repository
```

Then select the repository again, or switch back to the gitcroc window, and gitcroc reads it again.

The same holds for hooks: git runs your repository's hooks as it does on the command line, and when a hook refuses a commit or a push, the job fails with the hook's output.

## Credentials and password prompts

gitcroc stores no git passwords, tokens or SSH keys. git gets them the way it always does: from your credential helper (such as Git Credential Manager on Windows, the macOS keychain helper, or libsecret on Linux) and your SSH agent.

When git needs an answer it cannot get on its own, gitcroc shows git's question in a dialog that names the job and the repositories it is for: a user name, a password or passphrase (typed hidden), or whether to trust an SSH host key you have not seen before (**Yes** or **No**). **OK** gives git your answer; **Cancel** or Escape gives it none, and the operation fails. gitcroc passes the answer to git and forgets it.

If gitcroc asks for the same password every time, set up a credential helper or add your key to your SSH agent, as you would for git on the command line.

## The keychain cannot be used

gitcroc keeps AI keys only in your system's keychain. When it cannot reach it, the **AI** tab of Settings says "The keychain cannot be used, so keys cannot be stored", with the reason, and providers that need a key cannot be saved with one. gitcroc never falls back to writing keys to a file.

- **Linux:** gitcroc uses the Secret Service, which GNOME Keyring and KWallet (with its Secret Service support turned on) provide. Install and start one of them, and make sure it is unlocked; on a desktop without one, or over SSH, there may be none.
- **macOS and Windows:** the Keychain and Credential Manager are always there. If one refuses, the reason in Settings comes from the system.

Providers that need no key, such as a local model or Claude Code and Codex, work without the keychain.

## Changes made outside gitcroc do not show

gitcroc watches the open workspace's repositories and reads them again when they change. If watching fails, for example because the system's limit on watched folders is reached, gitcroc says so: "Changes made outside gitcroc show only when the window regains focus". Switching to another window and back then reads every repository again. On Linux, raising `fs.inotify.max_user_watches` lifts the limit.

## A large repository is slow

For a repository with a long history and no commit-graph file, the history offers to write one (**Write commit-graph file**). For a repository with very many files, the changes pane offers to turn on git's own speed-ups for reading the status (**Turn on**), in that repository's config. Both are run by git and change nothing else. **Don't ask again** hides the offer for that repository.

## A settings or workspace file is read-only

If a file was written by a newer version of gitcroc, the older version opens it read-only and says so, rather than overwrite what it does not understand. Update gitcroc. If a settings file cannot be read at all, gitcroc starts with the defaults and saves nothing until you fix or remove the file; the message names the file. [Settings and files](settings-and-files.md#files) lists where the files are.

## Reporting a bug

Report bugs at [github.com/ericlass/gitcroc/issues](https://github.com/ericlass/gitcroc/issues). Please include:

- the gitcroc version, system and architecture, from the last line of **Settings**;
- the git version, from the **Git** section of Settings;
- what you did, what you expected and what happened instead;
- for a failed operation, git's output from the job list;
- `gitcroc.log` from your log folder: `~/.local/state/gitcroc/` on Linux, `~/Library/Logs/gitcroc/` on macOS, `%LOCALAPPDATA%\gitcroc\logs\` on Windows.

The log lists the git commands gitcroc ran, with your repository paths and branch names, but not your files' contents. If **AI debug logging** was on, the log also holds the code sent to the AI; look through it, or turn the setting off and reproduce the problem, before attaching it.
