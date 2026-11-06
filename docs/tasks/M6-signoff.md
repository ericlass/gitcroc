# M6 sign-off

Milestone M6 (docs/spec/13-milestones.md) is done when every UI requirement has a GPUI test, the section 9 budgets are met with the new scroll budget, and 1.0.0-rc.1 updates to 1.0.0-rc.2 on all three platforms. This file signs off the release candidate `1.0.0-rc.2` against that condition ([M6-31](M6-31-release-candidate-2.md)). The person verifying fills in this file and commits it.

Anything this verification finds wrong becomes a task numbered M6-31a, M6-31b and so on, is fixed before this sign-off ends, and is listed under [Follow-up tasks](#follow-up-tasks). A fix that changes a package after rc.2 is published means a new release candidate (`rc.3`), and the release, the version order and the by-hand checks below are repeated for it.

## Status

Not yet verified. Windows and macOS: install and update are covered by CI (below); budgets, the NVDA and VoiceOver scripts and the by-hand checks are deferred, as decided for M0 on 2026-10-12 and for M6-31 on 2026-11-06 (no Windows PC or Mac).

## Requirement coverage (QA-3)

`just check-rust` runs `just coverage M6` and `just coverage spec`, both enforcing: either fails `just check` when a requirement has no test. Only Rust tests under `crates/` count (M6-30). ADR 0003's four rules need no test.

- [ ] `just coverage spec` on the tagged commit: ______ covered, 4 no test needed (DEV-1 to DEV-4), 0 missing.
- [ ] `just coverage M6` on the tagged commit: ______ covered, ______ no test needed, 0 missing.

## Version order

Each package orders `1.0.0-rc.2` after `1.0.0-rc.1` and before `1.0.0`. The `.deb` and the `.rpm` carry epoch 1 from rc.2 on, because rc.1's have none and sort after `1.0.0` (Eric, 2026-11-06). Checked by ______, date ______.

| Package | How the version is written | rc.1 to rc.2 | rc.2 to `1.0.0` | Result |
| --- | --- | --- | --- | --- |
| Updater (`latest.json`) | `1.0.0-rc.2`, compared as semver | | | |
| Windows installer (NSIS) | | | | |
| macOS app | | not used for updates | not used for updates | |
| `.deb` | `1:1.0.0~rc.2`; rc.1's is `1.0.0-rc.1` | | | |
| `.rpm` | epoch 1, version `1.0.0~rc.2`, release 1; rc.1's is epoch 0, `1.0.0-rc.1`, release 1 | | | |

## The release

Pushing the tag and publishing are Eric's; an agent never does either ([RELEASING.md](../../RELEASING.md)).

- [ ] The pull request with the version and the changelog merged; CI, Build and Update test green on the merge commit: ______
- [ ] The release dry run on that pull request: ______ (link)
- [ ] Eric pushed the tag `v1.0.0-rc.2` on the merge commit: ______ (date)
- [ ] The Release workflow's run: ______ (link)
- [ ] The draft checked:
  - [ ] every file of RELEASING.md step 6, with `SHA256SUMS`
  - [ ] `latest.json`: version `1.0.0-rc.2`, the four platform keys `windows-x86_64`, `darwin-x86_64`, `darwin-aarch64` and `linux-x86_64`, each with a URL under `releases/download/v1.0.0-rc.2/` and a signature
  - [ ] one signature checked with minisign and the project's public key (RELEASING.md step 6)
  - [ ] the notes: the changelog section, then the download paragraph with the link to `docs/user/install.md`
  - [ ] the pre-release box ticked
- [ ] Published as a pre-release: ______ (link, date)

## CI proof on the tagged commit

The Windows and macOS proof ([docs/tasks/README.md](README.md), "Windows and macOS proof"). Each link is to the run on the tagged commit.

| Check | Task | Workflow, job | Result |
| --- | --- | --- | --- |
| Install on Windows, macOS and Linux | M6-28a | Build, `install (windows-x64)`, `install (macos-universal)`, `install (linux-x86_64)`, `install (rpm, fedora)` | ______ |
| The installer over rc.1 | M6-28a | Build, `install over rc.1 (windows-x64)` | ______ |
| The `.deb` and the `.rpm` over rc.1 | M6-31 | Build, the "upgrade rc.1" steps of `install (linux-x86_64)` and `install (rpm, fedora)` | ______ |
| GPUI A to B, and rc.1 to the GPUI build, on all three platforms | M6-28c | Update test, `update (…)` | ______ |
| Tests, smoke test on Linux and Windows, a11y tree on Linux, keychain on Windows and macOS | M6-25a, M6-29a | CI, `check-rust (…)` and `check (…)` | ______ |

## By hand on Linux, from the published pre-release

System: ______ (distribution, version, desktop). Date: ______

- [ ] The AppImage downloads and starts.
- [ ] The `.deb` or the `.rpm` installs and starts: ______ (which)
- [ ] rc.1's package, installed from its release, is upgraded by rc.2's with `apt` or `dnf`; the tabs, workspaces and settings are still there: ______
- [ ] The git version in Settings matches `git --version`: ______
- [ ] Settings, Updates runs a check. What it shows while no final release exists (`releases/latest` ignores pre-releases): ______
- [ ] The `latest.json` on the rc.2 release's page lists version `1.0.0-rc.2` and the four platform keys.

## Budgets (Linux, `target-project`)

Machine: ______ (M6-29's: AMD Ryzen 9 3900X pinned to 8 threads with `taskset -c 0-7`, 32 GB, Samsung SSD 970 EVO Plus, Ubuntu 24.04.5 LTS). git ______. Commit measured: ______. Measured by, date: ______

`taskset -c 0-7 just bench-ui --runs 3` and `taskset -c 0-7 just bench --runs 3`. The M6-29 column is [M6-29-measurements.md](M6-29-measurements.md), and for the group input latency [M6-29e](M6-29e-investigation.md). Pass is decided by the median.

| Scenario | M6-29 median | rc.2 median | rc.2 p95 | Unit | Budget | Pass |
| --- | ---: | ---: | ---: | --- | ---: | --- |
| `cold-start` | 734.602 | | | ms | 1000 | |
| `refresh-all-ui` | 836.856 | | | ms | 2000 | |
| `select-group` | 125.01 | | | ms | 300 | |
| `input-latency` | 20.778 | | | ms | 50 | |
| `input-latency-job` | 27.739 | | | ms | 50 | |
| `input-latency-group-job` | see M6-29e | | | ms | 50 | |
| `select-largest-repo` | 48.618 | | | ms | 200 | |
| `select-commit` | 90.283 | | | ms | 150 | |
| `switch-repo` | 48.62 | | | ms | 100 | |
| `switch-tab` | 13.898 | | | ms | 100 | |
| `memory-idle` | 300.472 | | | MB | 400 | |
| `memory-two-tabs` | 325.212 | | | MB | 400 | |
| `scroll-history-frame` | 6.999 | | | ms | 7.014 | |
| `scroll-history-janky` | 0.775 | | | % | 1.0 | |
| `scroll-diff-frame` | 6.945 | | | ms | 7.014 | |
| `scroll-diff-janky` | 0.0 | | | % | 1.0 | |
| `scroll-blank` | 0.0 | | | ms | 100 | |
| `switch-tab-history` | 204.883 | | | ms | reported | — |

| Data set | Scenario | M6-29 median | rc.2 median | rc.2 p95 | Budget (ms) | Pass |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| `target-project` | `refresh-all` | 794.563 | | | 2000 | |
| `target-project` | `history-first-page` | 21.573 | | | 200 | |
| `target-project` | `group-history-first-page` | 67.288 | | | 300 | |
| `target-project` | `select-commit` | 13.246 | | | 150 | |
| `target-project` | `status-60k` | 74.626 | | | 500 | |

### Stretch results (reported, not gating)

From `taskset -c 0-7 just bench --runs 3 --stretch`, core side only (PERF-8).

| Set | Scenario | M6-29 median | rc.2 median | Stretch target | Within |
| --- | --- | ---: | ---: | --- | --- |
| `many-repos` | `cold-start-core` | 5.518 ms | | 1.5 s | |
| `many-repos` | `refresh-all` | 2753.57 ms | | 5 s | |
| `big-history` | `history-first-page` | 98.882 ms | | 300 ms | |
| `big-tree` | `status-big-tree` | 217.086 ms | | 1 s | |

## Accessibility (Linux, Orca, by hand)

The Orca script in [`crates/gitcroc-ui/tests/a11y_tree/manual.md`](../../crates/gitcroc-ui/tests/a11y_tree/manual.md), on X11 and on Wayland. Orca ______, desktops ______. Date: ______

| Step | X11: what Orca read | Wayland: what Orca read | Notes |
| --- | --- | --- | --- |
| 1 Window opens | | | |
| 2 History rows, position, selected | | | |
| 3 Tab order, window buttons | | | |
| 4 Tree | | | |
| 5 Dialog: opens, Tab, Escape, focus back | | | |
| 6 Commit message by character and line | | | |
| 7 Failure announced once | | | |

- [ ] The focus ring is seen on each Tab stop of step 3, in the light and the dark theme (README "Accessibility bar in M6").

## Keyboard only (by hand)

The mixed-branch scenario of section 5, from start to end without the mouse. Date: ______

- [ ] Select the group, check out `main`: every repository switches, and the group shows `main`.
- [ ] Select one repository, check out `feature/x`: only that one switches.
- [ ] The group shows "mixed (2)", and each repository row its own branch.
- [ ] Group pull: each repository pulls the branch it is on.
- [ ] Group checkout `main` brings every repository back to `main`.

## Documentation

Each page of `docs/user/` read against rc.2, its screenshots matching the app. Date: ______

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
| Install, install over rc.1 and update | CI, above |
| Budgets, NVDA, keyboard and documentation by hand | deferred |

## macOS on Apple silicon (deferred)

| Check | Result |
| --- | --- |
| Install and update | CI, above |
| Budgets, VoiceOver, keyboard and documentation by hand | deferred |

## Follow-up tasks

| Task | Found | Changes a package |
| --- | --- | --- |
