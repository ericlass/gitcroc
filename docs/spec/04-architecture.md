# 4. Architecture and tech stack

The app is a Rust core that owns all git, job and AI logic, with a native UI in Rust and GPUI on top that only renders state and calls the core's services.

```text
+--------------------------------------------------------------------+
| gitcroc-ui (GPUI views)                                            |
| renders state and calls services; no business logic, no truth      |
+--------------------------------------------------------------------+
        ^
        |  in-process calls down; state and progress events up an
        |  event channel
        v
+----------------------------------------------+  +------------------+
| gitcroc-app                                  |  | gitcroc-cli      |
| services, platform, askpass, updater         |  | headless: tests, |
|                                              |  | bench            |
+----------------------------------------------+  +------------------+
        |                                                  |
        v                                                  v
+ - - - Rust crates with no UI toolkit (ARCH-5) - - - - - - - - - - -+
|                                                                    |
|  +-----------------------------------+   +----------------------+  |
|  | gitcroc-core                      |   | gitcroc-ai           |  |
|  | workspaces, groups, jobs,         |   | providers, prompts,  |  |
|  | preflight, reports, cache         |   | response validation  |  |
|  +-----------------------------------+   +----------------------+  |
|          |                                          |              |
|          v                                          |              |
|  +-----------------------------------+              |              |
|  | gitcroc-git                       |              |              |
|  | read API, CLI runner, parsers,    |              |              |
|  | conflict detection                |              |              |
|  +-----------------------------------+              |              |
+ - - - - |  - - - - - - - | - - - - - - - - - - - - -|- - - - - - - +
          v                v                          v
   +-------------+  +------------------------+  +---------------------+
   | gix library |  | git CLI                |  | AI providers        |
   | reads       |  | all writes and network |  | the user's API      |
   |             |  |                        |  | endpoints and CLIs  |
   +-------------+  +------------------------+  +---------------------+
```

`gitcroc-app` and `gitcroc-cli` both sit on `gitcroc-core` and `gitcroc-ai`; the Crates table below is the binding list of allowed dependencies.

The shaded crates hold all logic and can be driven without a window, which is what makes them testable by agents.

## Why hybrid git (D3)

As of October 2026, gitoxide still lists push, merge and rebase as unfinished in its [crate status](https://github.com/GitoxideLabs/gitoxide/blob/main/crate-status.md). So `gix` is used where it is strong (reading refs, walking history, reading objects, diff, status) and the `git` CLI does everything that changes a repo or touches the network.

- **ARCH-1** Every operation that modifies a repo or contacts a remote MUST run through the `git` CLI. The single exception is writing the resolved file at Mark resolved (OPS-5), which git then stages.
- **ARCH-2** Reads SHOULD use `gix`. Where `gix` lacks a feature or gives a different answer than git, the read falls back to the CLI behind the same Rust trait, so callers never know which was used.
- **ARCH-3** The CLI is always invoked with an argument array, never through a shell. Paths follow a `--` separator. Output is parsed from porcelain or `-z` formats only. `LC_ALL=C` and `GIT_TERMINAL_PROMPT=0` are set.
- **ARCH-4** At most one mutating git process runs per repo at a time. Reads may run concurrently with it.

## Stack

| Layer | Choice | Notes |
| --- | --- | --- |
| Shell | GPUI, through gpui-kit and gpui-component, at pinned exact versions | Direct3D 11 on Windows, Metal on macOS, Vulkan or GL through wgpu on Linux |
| Core language | Rust stable, edition 2024 | `tokio` for async and process handling |
| Git reads | `gix` | Uses commit-graph files when present |
| Git writes | System `git` 2.40+ (D13) | Detected at startup; a setup screen appears if missing or too old |
| File watching | `notify` | See PERF-6 for what is watched |
| Local cache | SQLite via `rusqlite` | Disposable; deleting it loses nothing |
| Secrets | `keyring` | OS keychain (D11) |
| HTTP for AI | `reqwest` with rustls | Streaming responses |
| Front end | Rust GPUI views | View models are GPUI entities; no business logic |
| Lists | GPUI `uniform_list` and `list` | Every list and tree is virtualized |
| Diff and code view | GPUI views of our own, highlighted by `syntect` | Inline and side-by-side, themes, syntax highlighting |

## Crates

| Crate | Responsibility | May depend on |
| --- | --- | --- |
| `gitcroc-git` | Read API (`gix`), CLI runner, output parsers, conflict state detection | nothing internal |
| `gitcroc-core` | Workspace, group and repo model, job engine, preflight, result reports, config, cache | `gitcroc-git` |
| `gitcroc-ai` | Providers, prompt building, response validation | nothing internal |
| `gitcroc-app` | Services and their event channel; no UI toolkit. Askpass helper, window state, updater. Wires core and AI together | all of the above |
| `gitcroc-ui` | GPUI views and the `gitcroc` binary | all of the above |
| `gitcroc-cli` | Headless binary exposing core operations as JSON, used by tests and benchmarks | `gitcroc-core`, `gitcroc-ai` |

- **ARCH-5** `gitcroc-git`, `gitcroc-core` and `gitcroc-ai` MUST NOT depend on Tauri. Everything they do must be testable from `gitcroc-cli`. Only `gitcroc-ui` depends on GPUI.
- **ARCH-6** The UI holds no source of truth. After any job, the core emits fresh state and the UI re-renders from it.
- **ARCH-7** No work over 2 ms runs on the UI thread. Anything slower than 50 ms is a job that reports progress by events.
- **ARCH-8** Lists reach the UI in pages (history: 500 commits per page; diffs: one file at a time), so that memory stays bounded however large a repo is. Nothing unbounded is ever handed to the UI at once.

* **ARCH-9** All session state is keyed by workspace, in the core and in the UI: selection, history position, pane layout, conflict editor state, watchers. There is no global "current repo" or "current workspace" variable anywhere. This holds from M0, before tabs exist, so that tabs (UI-27) need no rework.

## Authentication prompts

Git asks for credentials through the helper and agent the user already has (D10). For prompts git cannot satisfy silently, the app sets `GIT_ASKPASS` and `SSH_ASKPASS` to a small helper binary that shows a native dialog and returns the answer to git. The answer is never stored by the app.
