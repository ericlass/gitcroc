# Tasks

One file per task. One task is one pull request and one agent session (DEV-4).
Work through a milestone in file order; each task lists what it depends on.

## Reading a task file

- **Implements** lists the spec IDs this task must fully satisfy. Each needs a test named after it (QA-3).
- **Respects** lists spec IDs the task must not break but does not complete.
- If the spec is silent or contradictory, stop and ask (DEV-2).

## M0 Skeleton

Milestone goal (docs/spec/13-milestones.md): an empty three-pane window opens on
all three platforms from CI artifacts, and QA-1 is green.

| Task | Delivers | Depends on |
| --- | --- | --- |
| [M0-01](M0-01-workspace-skeleton.md) | Cargo workspace with the five crates and lint rules | none |
| [M0-02](M0-02-quality-gates.md) | `just check` for Rust, cargo-deny, requirement coverage script | M0-01 |
| [M0-03](M0-03-ci.md) | CI running `just check` on Windows, macOS and Linux | M0-02 |
| [M0-04](M0-04-tauri-solid-shell.md) | Tauri window with the SolidJS app and front-end checks | M0-03 |
| [M0-05](M0-05-typed-ipc.md) | Generated IPC types, first command and first event | M0-04 |
| [M0-06](M0-06-git-detection.md) | Git detection, version check, setup screen | M0-05 |
| [M0-07](M0-07-layout-theme-settings.md) | Three-pane layout, light and dark theme, settings file | M0-05 |
| [M0-08](M0-08-build-artifacts.md) | Installable CI artifacts and the M0 sign-off | M0-06, M0-07 |

M0-06 and M0-07 are independent of each other and can run in parallel.

## Choices made in these tasks that the spec does not cover

Each is a default so the tasks have no gaps. Veto any of them before M0-01
starts; a change after that becomes an ADR (DEV-2).

| Choice | Value | Used in |
| --- | --- | --- |
| Crate location | `crates/<name>/`, front end in `ui/` | M0-01 |
| Rust toolchain | Pinned stable version in `rust-toolchain.toml`, latest stable at the time of M0-01 | M0-01 |
| JS package manager | pnpm, Node LTS pinned in `.nvmrc` | M0-04 |
| Styling | Plain CSS with CSS variables, one stylesheet per component; no CSS framework | M0-04, M0-07 |
| App identifier | `dev.gitcroc.app` | M0-04 |
| Config directory name | `gitcroc` under the platform config, cache and log dirs | M0-07 |
| CI service | GitHub Actions | M0-03 |
| Task file headings | "Implements" and "Respects" inside the Spec section, so the coverage script knows which IDs a milestone must test | M0-02 |
