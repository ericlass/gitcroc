# M5 sign-off

Milestone M5 (docs/spec/13-milestones.md) is done when v1.0.0 installs and updates on all three platforms, and every requirement ID has a test (QA-3). This file signs off the release candidate `1.0.0-rc.1` against that condition, except the update from the release candidate to 1.0.0, which M5-30 checks. The person verifying fills in this file and commits it.

Anything this verification finds wrong becomes a task numbered from M5-21 (up to M5-29), is fixed before M5-30, and is listed under [Follow-up tasks](#follow-up-tasks). A fix that changes a package means a new release candidate (`rc.2`), and the release and by-hand checks below are repeated for it.

## Status

Not yet verified. Windows and macOS: install and update are covered by CI (below); budgets, screen readers and the by-hand checks are deferred, as decided for M0 on 2026-10-12 (no Windows PC or Mac to measure on).

## Requirement coverage (QA-3)

`just check-rust` runs `just coverage M5` and `just coverage spec`, both enforcing: either fails `just check` when a requirement has no test. ADR 0003's four rules need no test.

- [x] `just coverage spec`: 125 covered, 4 no test needed (DEV-1 to DEV-4), 0 missing. Branch `codex/m5-20-release-candidate`, 2026-10-21.
- [x] `just coverage M5`: 15 covered, 4 no test needed, 0 missing. Same branch and date.
- [ ] The same on the tagged commit: ______

## Version order

Before the version became `1.0.0-rc.1`, each package was checked to order it before `1.0.0`, so that the update to 1.0.0 is not seen as a downgrade. Checked by Claude for Eric Lass, 2026-10-21, against the Tauri CLI 2.12.2 (tauri-bundler 2.10.2).

| Package | How the version is written | Order against `1.0.0` | Result |
| --- | --- | --- | --- |
| Updater (`latest.json`) | `1.0.0-rc.1`, compared as semver | before (SemVer 2.0.0, rule 11) | pass |
| Windows installer (NSIS) | Parsed as semver; `VIProductVersion` 1.0.0.0; the installer compares versions as semver | before | pass |
| macOS app | `CFBundleShortVersionString` and `CFBundleVersion` `1.0.0-rc.1`, as written | not used for updates | pass |
| `.deb` | `Version: 1.0.0-rc.1`, as written | after: `dpkg --compare-versions 1.0.0-rc.1 lt 1.0.0` is false | **fails** |
| `.rpm` | Version `1.0.0-rc.1`, release `1`, as written | after, by rpm's comparison rules (no rpm tools on the machine to run `rpmdev-vercmp`) | **fails** |

Tauri has no setting for a separate `.deb` or `.rpm` version. Eric's decision (2026-10-21): keep `1.0.0-rc.1` in every package, and tell `.deb` and `.rpm` users in the release candidate's notes (`CHANGELOG.md`, Known issues) to uninstall it before installing 1.0.0. The in-app updater on Linux updates the AppImage only, so it is not affected.

## The release

Pushing the tag and publishing are Eric's; an agent never does either ([RELEASING.md](../../RELEASING.md)).

- [ ] The pull request with the version and the changelog merged; CI green on the merge commit: ______
- [ ] Eric pushed the tag `v1.0.0-rc.1` on that commit: ______ (date)
- [ ] The Release workflow's run: ______ (link)
- [ ] The draft checked:
  - [ ] every file of RELEASING.md step 6, with `SHA256SUMS`
  - [ ] `latest.json`: version `1.0.0-rc.1`, the four platform keys `windows-x86_64`, `darwin-x86_64`, `darwin-aarch64` and `linux-x86_64`, each with a URL under `releases/download/v1.0.0-rc.1/` and a signature
  - [ ] the notes: the changelog section, then the unsigned-build paragraph with the link to `docs/user/install.md`
  - [ ] the pre-release box ticked
- [ ] Published as a pre-release: ______ (link, date)

## CI proof on the tagged commit

The Windows and macOS proof ([docs/tasks/README.md](README.md), "Windows and macOS proof"). Each link is to the run on the tagged commit.

| Check | Task | Workflow, job | Result |
| --- | --- | --- | --- |
| Install, Windows, macOS and Linux | M5-14 | Build, `install (…)` | ______ |
| Update from A to B, refused bad signature | M5-15 | Update test, `update` | ______ |
| Smoke test, Linux and Windows | M5-16 | CI, `smoke (…)` | ______ |
| OS keychain round trip, Windows and macOS | M5-16 | CI, `check-rust (…)` | ______ |

## By hand on Linux, from the published pre-release

System: ______ (distribution, version, desktop). Date: ______

- [ ] The AppImage downloads and starts.
- [ ] The `.deb` or the `.rpm` installs and starts: ______ (which)
- [ ] The git version in Settings matches `git --version`: ______
- [ ] Settings, Updates shows a check, and it finds nothing newer (`releases/latest` ignores pre-releases).

## Budgets (Linux, `target-project`)

Machine: ______ (the earlier sign-offs': AMD Ryzen 9 3900X pinned to 8 threads with `taskset -c 0-7`, 32 GB, Samsung SSD 970 EVO Plus, Ubuntu 24.04.5 LTS). WebKitGTK ______, git ______. Commit measured: ______. Measured by, date: ______

`taskset -c 0-7 just bench-ui --runs 3` and `taskset -c 0-7 just bench --runs 3`. The M4 column is the M4 sign-off's table. Pass is decided by the median, as before. Memory is included (M5-17).

| Section 9 row | Scenario | M4 median | M5 median | M5 p95 | Budget | Pass |
| --- | --- | --- | --- | --- | --- | --- |
| Cold start: tree visible from cache | `bench-ui` `cold-start` | 627 ms | | | 1 s | |
| Full status refresh of all 20 repos after start | `bench-ui` `refresh-all-ui` | 1,096 ms | | | 2 s | |
| Full status refresh, the core part | `bench` `refresh-all` | 854 ms | | | 2 s | |
| Select the largest repo: first page of history visible | `bench-ui` `select-largest-repo` | 97 ms | | | 200 ms | |
| Scroll history at any speed: 60 fps | `bench-ui` `scroll-frame` | 16 ms | | | 16.7 ms | |
| Scroll history: no row blank for more than 100 ms | `bench-ui` `scroll-blank` | 0 ms | | | 100 ms | |
| Select a commit with under 5,000 changed lines | `bench-ui` `select-commit` | 98 ms | | | 150 ms | |
| Switch selection between two repos: first paint | `bench-ui` `switch-repo` | 86 ms | | | 100 ms | |
| Switch between two tabs: first paint (UI-33) | `bench-ui` `switch-tab` | 84.5 ms | | | 100 ms | |
| Select a group of all 20 repos: first page of merged history | `bench-ui` `select-group` | 139 ms | | | 300 ms | |
| Select a group of all 20 repos, the core part | `bench` `group-history-first-page` | 75.5 ms | | | 300 ms | |
| Working copy status of the 60,000-file repo | `bench` `status-60k` | 81 ms | | | 500 ms | |
| UI input latency while any job runs: a refresh | `bench-ui` `input-latency` | 29.5 ms | | | 50 ms | |
| UI input latency while any job runs: fetch and pull | `bench-ui` `input-latency-job` | 16 ms | | | 50 ms | |
| UI input latency while any job runs: a group fetch and a group pull | `bench-ui` `input-latency-group-job` | 24 ms | | | 50 ms | |
| Memory, workspace idle, including the webview | `bench-ui` `memory-idle` | 441 MB | | | 400 MB | |
| Memory, with a second tab in the background (UI-31) | `bench-ui` `memory-two-tabs` | 456 MB | | | 400 MB | |

### Stretch results (reported, not gating)

From `taskset -c 0-7 just bench --runs 3 --stretch`, core side only (PERF-8). Memory with `big-history` scrolled 100,000 rows (stretch target 1.2 GB) is not measured: `just bench-ui` runs on `target-project` only.

| Set | Scenario | Measured (median) | Stretch target | Within |
| --- | --- | --- | --- | --- |
| `many-repos` | `cold-start-core` | | 1.5 s (the tree visible) | |
| `many-repos` | `refresh-all` | | 5 s | |
| `big-history` | `history-first-page` | | 300 ms | |
| `big-tree` | `status-big-tree` | | 1 s | |

## Accessibility (Linux, Orca, by hand)

Orca ______, desktop ______. Date: ______. Note what Orca announces at each step.

| Step | Announcement | Result |
| --- | --- | --- |
| Open a workspace | | |
| Move through the tree | | |
| Move through the history | | |
| Stage a file and commit | | |
| Open the command palette and run Fetch on a group | | |
| Read the result report | | |
| Resolve a conflict with Take ours, then Mark resolved | | |

## Keyboard only (by hand)

The mixed-branch scenario of section 5, from start to end without the mouse. Date: ______

- [ ] Select the group, check out `main`: every repository switches, and the group shows `main`.
- [ ] Select one repository, check out `feature/x`: only that one switches.
- [ ] The group shows "mixed (2)", and each repository row its own branch.
- [ ] Group pull: each repository pulls the branch it is on.
- [ ] Group checkout `main` brings every repository back to `main`.

## Documentation

Each page of `docs/user/` read against the release candidate. Date: ______

- [ ] [README](../user/README.md)
- [ ] [Installing gitcroc](../user/install.md)
- [ ] [Getting started](../user/getting-started.md)
- [ ] [Workspaces and groups](../user/workspaces-and-groups.md)
- [ ] [Operations](../user/operations.md)
- [ ] [Conflicts and AI](../user/conflicts-and-ai.md)
- [ ] [Keyboard](../user/keyboard.md)
- [ ] [Settings and files](../user/settings-and-files.md)
- [ ] [Privacy](../user/privacy.md)
- [ ] [Troubleshooting](../user/troubleshooting.md)

## Windows (deferred)

| Check | Result |
| --- | --- |
| Install and update | CI, above |
| Budgets, screen reader, keyboard and documentation by hand | deferred |

## macOS on Apple silicon (deferred)

| Check | Result |
| --- | --- |
| Install and update | CI, above |
| Budgets, screen reader, keyboard and documentation by hand | deferred |

## Follow-up tasks

| Task | Found | Changes a package |
| --- | --- | --- |
| [M5-21](M5-21-changes-pane-stays-visible.md) The changes pane stays in the window | Eric, by hand on Linux, 2026-10-23 | yes |
| [M5-23](M5-23-own-title-bar.md) The tab bar is the title bar | Eric's request during the verification, 2026-10-23 | yes |
| [M5-24](M5-24-history-columns.md) History columns resize and move | Eric's request during the verification, 2026-10-23 | yes |
| [M5-25](M5-25-tree-names-stay-whole.md) A repo's name stays whole beside a long branch name | Eric, by hand on Linux, 2026-10-23 | yes |
| [M5-26](M5-26-palette-closes-on-outside-click.md) A click outside the command palette closes it | Eric, by hand on Linux, 2026-10-23 | yes |
