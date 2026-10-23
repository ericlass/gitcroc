# Settings and files

## Settings

Open **Settings** from the toolbar, with Ctrl+, (Cmd+, on macOS), or from the command palette. The settings are grouped in four tabs: **Appearance**, **Git**, **Updates** and **AI**. Click a tab, or move between them with the arrow keys. Every change is saved at once; there is no Save button. The last line, below every tab, shows which gitcroc you are running: its version, system and architecture.

### Appearance tab

#### Theme

**Follow the system** (the default), **Light** or **Dark**. Following the system switches with it while gitcroc runs.

#### Code themes

The colors of diffs and of the conflict editor, chosen apart from the app theme: **Code theme in light mode** (GitHub Light by default) and **Code theme in dark mode** (GitHub Dark by default). Each choice shows a preview. The themes are GitHub Light, GitHub Dark, Solarized Light, Solarized Dark, One Dark, Dracula, Nord and Monokai.

#### Fonts

- **UI font**: the font of the window. Leave it empty for the system UI font.
- **UI font size**: in pixels, 8 to 32; 13 by default.
- **Code font**: the font of diffs and the conflict editor. Leave it empty for the system monospace font.
- **Code font size**: in pixels, 8 to 32; 13 by default.

#### Window

**Use the system title bar**: show your system's title bar and window buttons above the tab bar. Off by default: the tab bar is the title bar, as described in [Getting started](getting-started.md). Turn it on if your desktop, such as a tiling window manager, works better with the system's own frame. The change applies at once.

### Git tab

#### Git

Shows the git gitcroc uses, with its version and path. **Choose git…** picks another git program; **Use automatic detection** goes back to finding git by itself. See [Troubleshooting](troubleshooting.md#git-not-found-or-too-old).

#### Group operations

- **Repos at once**: how many repositories a group operation works on at the same time, 1 to 32; 8 by default.
- **Always show the preflight dialog**: show the check before every group operation, even when every repository will run. Off by default. See [Preflight and confirmation](operations.md#preflight-and-confirmation).

### Updates tab

- **Check for updates automatically**: on by default. See [Privacy](privacy.md#the-update-check).
- **Check now**: checks once, also while automatic checks are off. The line above the button shows the last result and when it was.

A build you made yourself says "This build does not check for updates; only release builds do."

### AI tab

- **AI features**: turns every AI feature on or off. On by default. While it is off, the rest of this tab is hidden.
- **Providers**: the AI providers you set up, with **Add provider…**, **Edit…**, **Set as default** and **Remove…**.
- **Never send files matching (one pattern per line)**: files never sent to an AI. It starts with `.env*`, `*.pem`, `*.key`, `*.p12` and `id_rsa*`.
- **AI debug logging**: writes every AI request and answer to the log. Off by default; while on, the log holds your code.

[Conflicts and AI](conflicts-and-ai.md#ai-help) explains each of these.

### Remembered without asking

gitcroc also remembers these as you use them:

- for all workspaces: the diff mode (**Inline** or **Side by side**), **Ignore whitespace**, **Wrap lines**, **Context**, and whether the file list is **Flat** or a **Tree**;
- for each workspace: its pane sizes, where the changes pane is docked, and **AI for this workspace**;
- the open tabs, their order and the active tab.

## Files

gitcroc keeps everything in your user account's own folders. It never stores anything in your repositories: the only changes to a repository are those git makes when you run an operation.

| File | What it holds |
| --- | --- |
| `settings.toml` | Your settings, the list of AI providers (without keys) and the open tabs |
| `workspaces/<id>.toml` | One file per workspace: its name, repositories, groups and layout |
| `cache.db` | A summary of each repository, so the window shows them quickly at start. Safe to delete; gitcroc builds it again |
| `gitcroc.log` | The log: see below |

They live in these folders:

| System | `settings.toml` and `workspaces/` | `cache.db` | `gitcroc.log` |
| --- | --- | --- | --- |
| Linux | `~/.config/gitcroc/` | `~/.cache/gitcroc/` | `~/.local/state/gitcroc/` |
| macOS | `~/Library/Application Support/gitcroc/` | `~/Library/Caches/gitcroc/` | `~/Library/Logs/gitcroc/` |
| Windows | `%APPDATA%\gitcroc\` | `%LOCALAPPDATA%\gitcroc\` | `%LOCALAPPDATA%\gitcroc\logs\` |

On Linux, `$XDG_CONFIG_HOME`, `$XDG_CACHE_HOME` and `$XDG_STATE_HOME` take the place of `~/.config`, `~/.cache` and `~/.local/state` when they are set.

AI keys are not in any of these files. They are in your system's keychain, under the service `gitcroc`.

### The log

`gitcroc.log` records every git command gitcroc runs, with its arguments, folder, duration and exit code, and gitcroc's own warnings. Its first line at each start names the version and the package. It never holds what git printed or the content of your files, unless you turn on **AI debug logging**, which writes the AI requests and answers. When the log reaches 10 MB, it is renamed `gitcroc.1.log` and a new one starts; the five newest files are kept.

### Workspace files

You do not need to edit workspace files by hand; use **Export workspace…** to share one (see [Sharing a workspace](workspaces-and-groups.md#sharing-a-workspace)). If a settings or workspace file comes from a newer version of gitcroc, it opens read-only and gitcroc says so: your changes are not saved, so the newer version's file stays intact. If a settings file cannot be read, gitcroc starts with the defaults, says what is wrong and leaves the file alone until you fix or remove it.

### Removing gitcroc completely

Uninstalling gitcroc leaves your settings, workspaces, cache and log in the folders above, so a new install finds them again. To remove them too, delete the `gitcroc` folders listed above and the keychain entries of the service `gitcroc`. The window's web view also keeps a folder named `dev.gitcroc.desktop` among your local application data; it holds nothing of yours and can go as well.
