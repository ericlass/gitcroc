# 10. Data, configuration, security and privacy

All app data lives in the per-user config, cache and log directories of the platform; nothing app-specific is ever stored inside a repo.

## Files

| File | Location | Content |
| --- | --- | --- |
| `settings.toml` | Config dir | App settings, provider list (without keys), theme choices |
| `workspaces/<uuid>.toml` | Config dir | One workspace: name, repos, groups, pane layout |
| `cache.db` | Cache dir | Repo summaries (PERF-3). Safe to delete |
| `gitcroc.log` | Log dir | Rotating, 5 files of 10 MB. Git commands run and their exit codes; no file contents |

A workspace file looks like this:

```toml
schema = 1
name = "Platform"

[[repo]]
id = "r1"
path = "/home/eric/src/platform/api"
remote = "git@example.com:platform/api.git"

[[repo]]
id = "r2"
path = "/home/eric/src/platform/web"
remote = "git@example.com:platform/web.git"

[[group]]
name = "Backend"
repos = ["r1"]

[[group]]
name = "Everything"
repos = ["r1", "r2"]
```

- **DATA-1** Config files are written atomically (write to a temporary file, then rename). Each has a `schema` number; older schemas are migrated forward on load, and a newer schema than the app knows is opened read-only with a warning.
- **DATA-2** **Export workspace** writes the same format with paths relative to the exported file. **Import workspace** lists repos whose path is missing and offers to clone them from `remote`. This is how a team shares a workspace.
- **DATA-3** The app never creates files inside a repo or its `.git` directory, apart from what git itself writes and the exception in ARCH-1.

## Security

- **SEC-1** AI keys are stored only in the OS keychain (D11). They never appear in config files, logs, IPC payloads to the UI, or error messages.
- **SEC-2** The webview loads only bundled assets, under a content security policy that forbids remote scripts, styles and frames. Tauri capabilities expose only the app's own commands; the UI cannot run arbitrary processes or read arbitrary files.
- **SEC-3** Everything from a repo is untrusted input: file names, commit messages, branch names, diff content. It is rendered as text, never as HTML, and never interpolated into a command line (ARCH-3).
- **SEC-4** Branch and tag names typed by the user are validated against git's ref format rules in the core before any command runs.
- **SEC-5** Repo text placed in an AI prompt is wrapped as clearly delimited data. A prompt injection can at worst produce a bad proposal, which the user reviews (D9, AI-9).
- **SEC-6** Git runs repo hooks exactly as on the command line. Git's own safety errors (for example `safe.directory`) are shown to the user and never bypassed by the app.
- **SEC-7** Windows and macOS builds are not code-signed or notarized in v1, so the OS warns about an unidentified developer on first launch; the download page explains how to proceed. macOS builds are ad-hoc signed, which Apple silicon requires to run at all. Update packages are still verified with the project's own updater key, which costs nothing. Paid code signing is LATER. The update check is the only network call the app makes on its own (D12) and can be turned off.
