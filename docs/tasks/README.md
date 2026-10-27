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

## M1 Read-only single repo

Milestone goal (docs/spec/13-milestones.md): the single-repo budgets of section 9
are met on `target-project`, the stretch results for `big-history` and
`big-tree` are reported, and no write operation exists yet.

| Task | Delivers | Depends on |
| --- | --- | --- |
| [M1-01](M1-01-reference-data-and-bench.md) | Fixture helpers, reference data generator, `just bench` harness | M0 |
| [M1-02](M1-02-history-spike.md) | Spike: paged history walk with graph lanes and search in the core | M1-01 |
| [M1-03](M1-03-diff-spike.md) | Spike: commit header, changed files and one file's diff in the core | M1-01 |
| [M1-04](M1-04-status-and-repo-state.md) | Working copy status, uncommitted diffs, conflict state, repo summary | M1-03 |
| [M1-05](M1-05-workspace-and-add-repo.md) | Workspace file, add repo and scan a folder, remove repo | M1-01 |
| [M1-06](M1-06-summary-cache.md) | `cache.db`, stale summaries at start, background refresh | M1-02, M1-04, M1-05 |
| [M1-07](M1-07-tree-pane.md) | Tree pane with refs, repo rows, filter, add repository | M1-06 |
| [M1-08](M1-08-history-pane.md) | History pane with graph, paging, uncommitted row, search | M1-02, M1-07 |
| [M1-09](M1-09-changes-pane.md) | Changes pane: header, file list, inline and side-by-side diff, docking | M1-03, M1-04, M1-08 |
| [M1-10](M1-10-code-themes-and-fonts.md) | Eight code themes with contrast test, fonts | M1-09 |
| [M1-11](M1-11-file-watching.md) | File watching and refresh on change and focus | M1-07, M1-08, M1-09 |
| [M1-12](M1-12-budgets-and-signoff.md) | Nightly bench with regression gate, UI-side measurement, M1 sign-off | M1-01 to M1-11 |
| [M1-13](M1-13-input-latency-during-refresh.md) | Input latency within 50 ms while a refresh runs (missed in M1-12) | M1-12 |
| [M1-14](M1-14-idle-memory.md) | Idle memory within 400 MB (missed in M1-12) | M1-12 |
| [M1-15](M1-15-cold-start-investigation.md) | Investigation: can the cold start reach 1 s, and how | M1-12 |

M1-13 to M1-15 come from the budgets M1-12 found missed; M1 is signed off after
them. They are independent of each other.

M1-02 and M1-03 are the spikes of section 14 and run first: they carry the
performance risk. If either misses its budget, M1 stops for a decision on D2
and D3 (section 13) before the UI tasks start. M1-02, M1-03 and M1-05 can run in
parallel after M1-01.

## M2 Single-repo writes

Milestone goal (docs/spec/13-milestones.md): every operation in section 6 works
on a single repo, through the job engine, with the status bar and job list, the
askpass helper, and the manual conflict editor (OPS-1 to OPS-9). Done when each
operation has integration tests for success, failure and conflict, and a merge
conflict can be resolved and continued by hand.

| Task | Delivers | Depends on |
| --- | --- | --- |
| [M2-01](M2-01-job-engine.md) | Job engine: per-repo write queue, outcomes, cancel, re-read after a job, shared pool, `gitcroc.log` | M1 |
| [M2-02](M2-02-status-bar-and-job-list.md) | Status bar with running jobs, job list with outcomes and git's output | M2-01 |
| [M2-03](M2-03-askpass.md) | Askpass helper: credential and host-key prompts in a dialog, never stored | M2-01 |
| [M2-04](M2-04-stage-unstage-discard.md) | Stage, unstage and discard by file and by hunk | M2-01 |
| [M2-05](M2-05-commit-and-amend.md) | Commit message box, commit and amend, hooks as on the command line | M2-04 |
| [M2-06](M2-06-branches.md) | Checkout, create, rename, delete, upstream, remote and detached checkout; tree context menus | M2-01, M2-03 |
| [M2-07](M2-07-fetch-pull-push.md) | Fetch, pull, push with progress; toolbar | M2-02, M2-03, M2-06 |
| [M2-08](M2-08-merge-rebase-cherry-pick-revert.md) | Merge, rebase, cherry-pick, revert, abort and continue; conflict banner; commit context menu | M2-06 |
| [M2-09](M2-09-conflict-model.md) | Conflict hunks from the index stages, side labels, Mark resolved, choice conflicts (core) | M2-08 |
| [M2-10](M2-10-conflict-editor.md) | Conflict editor with take ours, theirs and both; unsaved state kept until Abort | M2-04, M2-09 |
| [M2-11](M2-11-stash.md) | Stash, apply, pop, drop; stash apply conflicts | M2-10 |
| [M2-12](M2-12-tags-and-reset.md) | Create, delete and push tags; reset soft, mixed, hard | M2-07, M2-08 |
| [M2-13](M2-13-remotes-and-fsmonitor-offer.md) | Add, rename, remove remotes and edit URLs; the PERF-7 offer | M2-07 |
| [M2-14](M2-14-clone-and-open.md) | Clone with progress; open in file manager and in terminal | M2-03, M2-07 |
| [M2-15](M2-15-signoff.md) | Operation test matrix, end-to-end merge conflict test, M2 sign-off | M2-01 to M2-14 |

M2-02, M2-03 and M2-04 can run in parallel after M2-01, and M2-05 and M2-06
after them. M2-09 is the risky part of the milestone (hunks from index stages,
detecting a file edited outside); it can start as soon as M2-08's conflicts
exist, while M2-12 to M2-14 run beside it.

## M3 Multi-repo

Milestone goal (docs/spec/13-milestones.md): workspaces in tabs, groups,
multi-selection, preflight, group operations including group commit, the
result report, merged history, the summary strip, and import and export. Done
when the mixed-branch scenario passes as a test, all section 9 budgets are met
on `target-project`, and the stretch results for `many-repos` are reported.

| Task | Delivers | Depends on |
| --- | --- | --- |
| [M3-01](M3-01-workspaces.md) | Several open workspaces in the core: list, create, open, close, rename, delete, session restore | M2 |
| [M3-02](M3-02-workspace-tabs.md) | Tab bar with the "+" menu, reorder and close; each tab restores its state | M3-01 |
| [M3-03](M3-03-background-tabs.md) | Background tabs not watched or refreshed, history and diffs released, tab badges | M3-02 |
| [M3-04](M3-04-groups-core.md) | Groups in the core: names, members, order, derived branch and totals, tree with groups | M3-01 |
| [M3-05](M3-05-groups-in-the-tree.md) | Group rows, group context menus, dragging repos between groups | M3-04 |
| [M3-06](M3-06-target-sets.md) | Multi-selection, target sets, toolbar on the target set, summary strip | M3-05 |
| [M3-07](M3-07-merged-history.md) | Merged history: lazy k-way merge, repo column, search; group history budget | M3-06 |
| [M3-08](M3-08-group-jobs-and-preflight.md) | Group jobs with sub-jobs, preflight, confirm rule, result report, parallelism (core) | M3-06 |
| [M3-09](M3-09-preflight-dialog-and-report.md) | Preflight dialog, destructive confirmation, result report, Retry failed | M3-08 |
| [M3-10](M3-10-group-sync.md) | Group fetch, pull and push | M3-09 |
| [M3-11](M3-11-group-branches.md) | Group checkout, create and delete branch; group Branches double-click; mixed-branch scenario | M3-10 |
| [M3-12](M3-12-group-merge-rebase-tag.md) | Group merge and rebase with conflict prediction; group create tag | M3-09 |
| [M3-13](M3-13-group-commit.md) | Group commit: sectioned uncommitted view, one message, one commit per repo | M3-07, M3-09 |
| [M3-14](M3-14-import-export.md) | Export and import of workspaces, cloning missing repos, clone into a group | M3-02, M3-04 |
| [M3-15](M3-15-signoff.md) | Multi-repo budgets, many-repos stretch, group smoke test, M3 sign-off | M3-01 to M3-14 |

Two tracks can run side by side after M3-01: tabs (M3-02, M3-03) and groups
(M3-04 onward). M3-07 and M3-08 can run in parallel after M3-06, and M3-12
beside M3-10 and M3-11. M3-14 needs only M3-02 and M3-04. M3-08 is the core of
the milestone: every group operation is built on its classify and run parts.

## M4 AI conflict resolution

Milestone goal (docs/spec/13-milestones.md): providers, settings, consent,
proposals in the conflict editor, Resolve all, CLI providers. Done when all AI
requirements have tests against the mock provider, and the fixture set (AI-15)
runs against one API provider and one CLI provider.

| Task | Delivers | Depends on |
| --- | --- | --- |
| [M4-01](M4-01-response-validation.md) | Response model, validation, one retry, "AI failed"; provider trait and mock provider | M3 |
| [M4-02](M4-02-prompt-building.md) | Versioned prompt files, data delimiting, temperature 0, splitting over the context budget | M4-01 |
| [M4-03](M4-03-api-providers.md) | Anthropic and OpenAI-compatible providers, model lists, token usage | M4-01 |
| [M4-04](M4-04-cli-providers.md) | Claude Code and Codex CLI providers in an empty temp folder with tools off; self-test | M4-01 |
| [M4-05](M4-05-provider-settings-and-keys.md) | Provider list and AI settings in `settings.toml`, keys in the OS keychain, provider IPC | M4-03, M4-04 |
| [M4-06](M4-06-provider-settings-ui-and-consent.md) | AI section in Settings, consent dialog, workspace AI switch | M4-05 |
| [M4-07](M4-07-ai-eligibility-and-context.md) | Eligibility and exclude list; the data sent, read from the repo (core) | M3 |
| [M4-08](M4-08-proposal-jobs.md) | Proposal jobs, proposals in memory, the core and AI seam, AI debug logging | M4-02, M4-05, M4-07 |
| [M4-09](M4-09-proposals-in-the-conflict-editor.md) | Resolve with AI per file; Accept, Reject, Edit; Accept all high-confidence; token usage | M4-06, M4-08 |
| [M4-10](M4-10-resolve-all.md) | Resolve all with AI from the banner: 4 files at a time, progress, cancel | M4-09 |
| [M4-11](M4-11-prompt-fixture-set.md) | At least 30 reviewed fixture conflicts and `just ai-eval` | M4-02, M4-03, M4-04, M4-07 |
| [M4-12](M4-12-signoff.md) | Fixture runs against an API and a CLI provider, AI smoke test, M4 sign-off | M4-01 to M4-11 |
| [M4-13](M4-13-idle-memory.md) | Idle memory within 400 MB again (missed in M4-12) | M4-12 |

M4-07 needs nothing from the other M4 tasks and can run beside M4-01 to M4-04.
M4-03 and M4-04 can run in parallel after M4-01, and M4-11 beside M4-08 to
M4-10. M4-08 is the core of the milestone: it joins the core and `gitcroc-ai`
and carries D9.

## M5 Release

Milestone goal (docs/spec/13-milestones.md): installers, the updater, the
complete command palette, an accessibility and keyboard pass, user docs, and a
website or README. Done when v1.0.0 installs and updates on all three
platforms, and every requirement ID has a test (QA-3).

| Task | Delivers | Depends on |
| --- | --- | --- |
| [M5-01](M5-01-app-identifier.md) | App identifier `dev.gitcroc.desktop`, ADR 0002, section 15 closed | M4 |
| [M5-02](M5-02-spec-wide-coverage.md) | `just coverage spec` over every spec ID; DEV-1 to DEV-4 need no test (ADR 0003) | M4 |
| [M5-03](M5-03-decision-tests.md) | Tests for D1, D2, D3, D5, D6, D7, D8, D13 and D14 | M5-02 |
| [M5-04](M5-04-strings-shell-and-tree.md) | Strings module and its test; shell, Settings, tree and tabs converted | M4 |
| [M5-05](M5-05-strings-panes.md) | History, changes, conflicts, AI and jobs converted | M5-04 |
| [M5-06](M5-06-strings-dialogs.md) | Operation dialogs and stores converted; D15 complete | M5-05 |
| [M5-07](M5-07-palette-core.md) | Palette catalog, matching and ranking in the core | M4 |
| [M5-08](M5-08-palette-ui.md) | Palette window: Ctrl/Cmd+K, run an operation, select a repo, group or branch | M5-06, M5-07 |
| [M5-09](M5-09-keyboard-pass.md) | Keymap, every operation reachable without a mouse from its own place | M5-08 |
| [M5-10](M5-10-accessibility-pass.md) | axe checks in both themes, app theme contrast, focus, live region, motion | M5-09 |
| [M5-11](M5-11-git-path-setting.md) | Git path in Settings and on the setup screen | M5-06 |
| [M5-12](M5-12-updater.md) | Update check, notice, install and restart or download page; setting | M5-01, M5-10, M5-11 |
| [M5-13](M5-13-release-workflow.md) | Tag-triggered release workflow, signed update packages, `latest.json`, draft release | M5-12 |
| [M5-14](M5-14-install-tests.md) | Every package installed and started on its CI runner | M5-13 |
| [M5-15](M5-15-update-tests.md) | Update, bad signature and notify-only tested on the CI runners | M5-14 |
| [M5-16](M5-16-windows-smoke-and-keychains.md) | Smoke test on Windows; OS keychain test on Windows and macOS | M4 |
| [M5-17](M5-17-idle-memory.md) | Idle memory within 400 MB, one and two tabs | M5-08 to M5-12 |
| [M5-18](M5-18-user-docs.md) | User documentation in `docs/user/`, link test | M5-09, M5-11, M5-12, M5-13 |
| [M5-19](M5-19-readme.md) | README as front page and download page | M5-18 |
| [M5-20](M5-20-release-candidate.md) | 1.0.0-rc.1, coverage enforcing over the whole spec, M5 sign-off | M5-01 to M5-19 |
| [M5-30](M5-30-v1-0-0.md) | v1.0.0 released; the release candidate updates to it | M5-20 and its follow-ups, M6-31 |

M5-01, M5-02, M5-04, M5-07 and M5-16 need nothing from other M5 tasks and can
start together; M5-01 goes first among them, before anything is released. The
string tasks (M5-04 to M5-06) touch every UI area, so the other UI tasks wait
for them. M5-11 can run beside M5-07 to M5-10, and M5-12 follows it, since both change Settings. Follow-ups found by the release
candidate are numbered M5-21 to M5-29, so they come before M5-30 in file order.

M5-30 is held until M6 ends: v1.0.0 ships with the GPUI UI, after 1.0.0-rc.2
(decision 1 of `docs/plans/native-ui-gpui.md`, 2026-10-26).

## M6 Native UI

Milestone goal (`docs/plans/native-ui-gpui.md`; section 13 once M6-01 is
merged): the whole UI is rewritten in Rust with GPUI, and Tauri and the web
front end are removed. Done when every UI requirement has a GPUI test, the
section 9 budgets are met with the new scroll budget, and 1.0.0-rc.1 updates
to 1.0.0-rc.2 on all three platforms.

The Tauri app is deleted in M6-05; 1.0.0-rc.1 stays the released app until
rc.2. Until M6-30 the new UI builds as `gitcroc-next`, and until M6-28 it is
not packaged. Task files from M6-05 on are written
once the spikes M6-02 to M6-04 have reported, because their findings shape
M6-06, M6-19, M6-25, M6-28 and M6-29. M6-05 to M6-07 and M6-27 have theirs;
the rest follow in batches by lane. Rows marked † will be split into
several pull requests (DEV-4).

| Task | Delivers | Depends on |
| --- | --- | --- |
| [M6-01](M6-01-spec-and-adr.md) | Spec edits for GPUI, ADR 0004 | none |
| [M6-02](M6-02-conflict-editor-spike.md) | Spike: the conflict editor on gpui-component's editor, or its fallback | none |
| [M6-03](M6-03-ci-and-packaging-spike.md) | Spike: GPUI tests and windows on the hosted runners; packages without Tauri | none |
| [M6-04](M6-04-accessibility-spike.md) | Spike: screen readers and accessibility tests with GPUI | none |
| [M6-05](M6-05-services-without-tauri.md) † | `gitcroc-app` services and event channel without Tauri; the Tauri shell, its workflows and the IPC layer are deleted (4 PRs) | M6-01 |
| [M6-06](M6-06-gpui-skeleton.md) | `gitcroc-ui` skeleton: window, own title bar, panes, status bar, strings, keymap, macOS menus, CI job, `cargo deny`, `just coverage M6` | M6-03, M6-05 |
| [M6-07](M6-07-theme-and-fonts.md) | App tokens, accent color, fonts, 8 code themes on `syntect`, contrast tests | M6-06 |
| M6-08 | Git setup screen and git path | M6-06 |
| M6-09 | Workspace tabs and workspace dialogs | M6-06 |
| M6-10 | Tree pane: rows, summaries, filter, selection | M6-09 |
| M6-11 † | History pane from the spike: columns, search, merged list, summary strip | M6-10 |
| M6-12 | Changes pane: commit header, file lists, parent selector | M6-11 |
| M6-13 † | Diff view: both modes, highlighting, wrap, options, binary and image, Load diff, docking | M6-07, M6-12 |
| M6-14 | `just bench-ui` on GPUI; single-repo budgets | M6-13 |
| M6-15 | Status bar, job list, notifications, askpass dialog | M6-06 |
| M6-16 | Stage, unstage and discard; commit and amend; fsmonitor offer | M6-13, M6-15 |
| M6-17 | Tree and history context menus; branch dialogs | M6-11, M6-15 |
| M6-18 † | Sync, integrate, stash, tag, reset and remote dialogs | M6-17 |
| M6-19 † | Conflict banner and conflict editor | M6-02, M6-16 |
| M6-20 | Clone, add folder, missing repos | M6-15 |
| M6-21 | Groups in the tree, drag between groups, group dialogs | M6-17 |
| M6-22 | Preflight dialog, result report, group commit | M6-16, M6-21 |
| M6-23 † | AI settings, providers, consent, proposals, Resolve all | M6-19 |
| M6-24 | Command palette and keyboard pass | M6-18 to M6-23 |
| M6-25 | Accessibility pass, to the bar decided after M6-04 | M6-04, M6-24 |
| M6-26 | Settings dialog, all four tabs | M6-07, M6-23 |
| [M6-27](M6-27-updater-without-tauri.md) | Updater without Tauri, proven by tests on every platform | M6-05 |
| M6-28 | Packaging and release workflow without Tauri; install tests; on the runners, the GPUI build updates itself and rc.1 updates to it | M6-03, M6-27 |
| M6-29 | Smoke test, idle memory, full budget run, user docs | M6-24 to M6-28 |
| M6-30 | The switch: `gitcroc` is the GPUI binary; `ui/`, specta and Node removed | `just coverage M6` empty |
| M6-31 | 1.0.0-rc.2 and the M6 sign-off | M6-30 |

M6-01 to M6-04 need nothing from each other and can start together. After
M6-06, three lanes touch separate files: M6-08; M6-09 to M6-14; and M6-15 to
M6-20.

### Spike findings

Each spike's crate and README stay on its own branch, never merged. These
notes are what later tasks need from them.

**M6-02, the conflict editor** (`spike/gpui-conflict`, `192aeb8`, 2026-10-26)
- **Decided** (Eric, 2026-10-27): M6-19 builds on gpui-component's code editor, in one buffer. The base and the AI proposal go in a card beside the hunk, not in a block under it.
- **Measured** on a 10,000-line, 50-hunk file at 144 Hz:
  - 143.5 fps;
  - CPU 2.9 ms a frame at p95;
  - 6.6 ms typing latency;
  - 22–29 ms from opening to first paint.
  The fallback of one block per hunk dropped 9–10% of frames and typed in 27.6 ms.
- **The editor supports 6 of the 10 items as they are.** The other 4 need workarounds:
  - **Hunk anchors:** an empty take needs a one-byte ghost anchor before the hunk.
  - **Conflict colours:** the editor's background is made transparent, and full-width rows are painted under it.
  - **Headers:** headers with their Take buttons are drawn over the marker lines, from the editor's scroll offset.
  - **Focus:** focus is given back when the editor is shown again.
- **Keep in M6-19:**
  - Soft wrap and folding off, because the overlays assume rows of one line height. Test overlay positions against `EditorState::range_to_bounds`.
  - Check IME composition with a composing input method on all platforms.
  - Find the one 50 ms frame when the end of the document first comes into view.
- **Split M6-19 into three PRs:**
  1. Buffer, anchors, takes, accept and undo, keys, read-back, kept state.
  2. Rows, headers, the base and AI proposal cards, and conflict colours.
  3. The banner, file switching, and Mark resolved and Continue through the job services.

**M6-03, CI and packaging** (`spike/gpui-ci`, `d40bb5a`; [run 1](https://github.com/ericlass/gitcroc/actions/runs/40455723553) and [run 2](https://github.com/ericlass/gitcroc/actions/runs/40457533100), 2026-10-27)
- **On the hosted runners (run 2, every job passed):**
  - **Headless tests:** `cargo test` passes on all three runners with no display and no GPU. ubuntu-latest needs `libfontconfig-dev libfreetype-dev`; their absence failed run 1.
  - **Real window, on all three runners:** Linux under Xvfb with lavapipe, and with GL on llvmpipe; Windows on WARP (Direct3D 11); macOS on Metal.
  - **Not Wayland:** under headless Weston GPUI panics, because the compositor has no `wl_seat`. CI uses Xvfb; Wayland is checked by hand.
  - **Packages:** NSIS (installed for the current user), the portable zip, the universal `.dmg` (ad hoc signed), the AppImage, the `.deb` and the `.rpm` all install and start.
  - **Fonts:** with no font installed, GPUI panics at its first text. The Linux packages must depend on a font, or the app must ship one (M6-07, M6-28).
  - **Update signatures:** the NSIS installer, `.app.tar.gz` and AppImage are signed in Tauri's format and verified with `minisign-verify`.
  - **Build times:** a cold release build takes 3 to 6.5 minutes and a cached one under 30 s. Packages are 6 to 14 MiB.
- **Proven locally:**
  - **Tests without a display:** headless GPUI tests run with no display and no GPU.
  - **Real window:** one runs under Xvfb with lavapipe, and wgpu falls back to GL on llvmpipe by itself.
  - **Linux packages:** the AppImage and `.deb` start. The `.rpm` builds.
  - **Update signatures:** signatures in Tauri's format are made and verified without Tauri.
- **Packages and dependencies:**
  - **Sizes:** the binary is 36.6 MiB stripped; the AppImage is 13.5 MiB.
  - **Runtime libraries:**
    - linked: `libxcb`, `libxkbcommon` and `libxkbcommon-x11`;
    - opened at run time: `libvulkan`, `libEGL` and the Wayland client libraries.
  - **Fonts:** no fontconfig or freetype.
  - **Release build:** it stays on ubuntu-22.04 for glibc 2.35.
- **For M6-06's `cargo deny`:**
  - **Licences:** allow 0BSD (enum-iterator), and add an exception for CC0-1.0 (tiny-keccak).
  - **Advisories:** ignore four "unmaintained" advisories, for instant, rustls-pemfile, rustybuzz and ttf-parser.
  - **HTTP client:** turn off gpui-kit's default `assets` feature if possible, since it brings in an HTTP client.
- **For M6-27 and M6-28:**
  - **NSIS template:** cargo-packager's NSIS installer finds the Tauri install's directory and registry keys, so it installs over it. It needs a custom template that adds Tauri's `/UPDATE` and `/ARGS` handling and the `MainBinaryName` value.
  - **Signing:** sign with our own step, which keeps the `version:` check, not with cargo-packager's signer.

**M6-04, accessibility** (`spike/gpui-a11y`, `e99c9bb`, 2026-10-26)
- **What Orca 46 reads on Linux X11:**
  - buttons, the checkbox, the selected history row and the status announcements;
  - dialogs keep focus correctly.
  Windows and macOS weren't tested; NVDA and VoiceOver scripts for Eric are in the README.
- **Gaps:**
  - **Tests:** `#[gpui::test]` cannot read the accessibility tree. A real window under Xvfb with an AT-SPI bus can, and only on Linux.
  - **Long lists:** only visible rows are exposed, so a row's name must carry its position.
  - **Text fields:** they don't expose their text.
  - **Tree rows on Linux:** AccessKit's Linux adapter has no expanded state or level for them.
  - **gpui-component widgets:** its tree, dialogs, title bar buttons and toasts lack names or roles.
  - **Not available:** an axe equivalent, and a forced-colours signal.
- **Open decision before M6-25:** the README proposes a bar of nine tested musts. Five points are lower than M5-10's, and they need a spec decision (DEV-2).

## Choices made in these tasks that the spec does not cover

Each is a default so the tasks have no gaps. Veto any of them before the first
task of its milestone starts; a change after that becomes an ADR (DEV-2).

| Choice | Value | Used in |
| --- | --- | --- |
| Crate location | `crates/<name>/`, front end in `ui/` | M0-01 |
| Rust toolchain | Pinned stable version in `rust-toolchain.toml`, latest stable at the time of M0-01 | M0-01 |
| JS package manager | pnpm, Node LTS pinned in `.nvmrc` | M0-04 |
| Styling | Plain CSS with CSS variables, one stylesheet per component; no CSS framework | M0-04, M0-07 |
| App identifier | `dev.gitcroc.desktop` (ADR 0002) | M0-04, M5-01 |
| How M6 is developed | On `main`, with no long-lived branch: the GPUI UI grows as `gitcroc-next` until M6-30 | M6-05 to M6-30 |
| Look of the GPUI UI | Today's tokens ported, plus the OS accent color and native window controls; not gpui-component's default theme | M6-07 |
| Code themes in M6 | Ported from `codeThemes.ts` as Rust data, with syntect themes built from it; not `.tmTheme` files, which have no word-level or conflict colours (Eric, 2026-10-27) | M6-07 |
| The OS accent and contrast | Its hue is kept and its lightness changed until every accent pair passes; the token accent when none does (Eric, 2026-10-27) | M6-07 |
| No font installed | The app exits with a message instead of GPUI's panic; the Linux packages depend on a font. No font is embedded (Eric, 2026-10-27) | M6-07, M6-28 |
| Spikes | Standalone crates under `spikes/`, outside the workspace, each on its own `spike/` branch and never merged | M6-02 to M6-04 |
| Conflict editor | gpui-component's code editor, one buffer; the base and the AI proposal in a card beside the hunk (Eric, 2026-10-27, after M6-02) | M6-19, M6-23 |
| The Tauri app during M6 | Not kept: 1.0.0-rc.1 is its last release. M6-05 deletes the shell, its workflows and the IPC layer; no app runs on `main` until M6-06, and none is released until M6-28 (Eric, 2026-10-27) | M6-05 |
| `ui/` during M6 | Stays in the tree as the reference the tasks port from, out of `just check` and CI, runnable with its fake core; M6-30 deletes it (Eric, 2026-10-27) | M6-05, M6-06, M6-30 |
| Updater proof | Tests on every platform in M6-27; on the runners in M6-28, with the GPUI packages: GPUI A to B, and rc.1 to the GPUI build | M6-27, M6-28 |
| `gitcroc-next` and user files | Reads and writes the same config, cache and workspace files as an installed rc.1; running both at once is not supported | M6-06 |
| Config directory name | `gitcroc` under the platform config, cache and log dirs | M0-07 |
| CI service | GitHub Actions | M0-03 |
| Task file headings | "Implements" and "Respects" inside the Spec section, so the coverage script knows which IDs a milestone must test | M0-02 |
| What M1 completes | M1 implements UI-1, UI-3 and UI-8 for repos under Ungrouped. UI-2, UI-4, UI-5 and UI-6 (groups, multi-selection) wait for M3; UI-7 and the controls of UI-15 (writes) wait for M2; PERF-4's shared job pool waits for M2's job engine, with a minimal refresh scheduler until then. OPS-1 (conflict detection, read-only) is done in M1 although section 13 lists it with M2 | M1-04, M1-06, M1-07, M1-09 |
| PERF-7 | Offering to enable `core.fsmonitor` changes git config, which is a write, so it is M2 | M1-04 |
| One workspace until M3 | The app opens the most recently modified workspace file, or creates "My workspace"; repos are named after their folder and get ids `r1`, `r2` | M1-05 |
| Budget rows in M1 | Single-repo rows: cold start, select the largest repo, scroll, select a commit, switch between two repos, status of the 60,000-file repo, input latency, memory. M3 rows: status of all 20 repos, tab switching, group history | M1-06, M1-12 |
| Measuring machine | The developer's Linux machine pinned to 8 cores, with its specs recorded; Windows and macOS measurements deferred as for M0 | M1-12 |
| Reference data generator | A tools crate `tools/repo-gen` that writes `git fast-import` streams with a fixed seed; it also holds the test fixture helpers, used as a dev-dependency. Data in `target/bench-data` or `$GITCROC_BENCH_DATA` | M1-01 |
| `big-history` | A clone of the Linux kernel at a fixed release tag; measured locally, never in CI | M1-01, M1-12 |
| `big-history` source | `git clone --single-branch --branch v6.17` of github.com/torvalds/linux, so only that tag's history is fetched and the data never changes | M1-01 |
| Generated repo shape | One quarter of the commits on side branches, every second side branch merged back; 30% of the files in the first commit, the rest added over time; a commit changes 1 to 20 files, 2% change 100 to 400; an annotated tag every 250 commits on `main`; 12 authors; repacked, with a commit-graph file, `main` checked out | M1-01 |
| Sizes the spec leaves open | `target-project`: the 19 smaller repos share 140,000 files and 1.4 million lines evenly. `big-tree`: 5 million lines, 5,000 commits, 30 branches. `many-repos`: 1,000 files and 10,000 lines per repo | M1-01 |
| Bench output | A JSON array, one object per scenario and set: `scenario`, `data_set`, `runs`, `median_ms`, `p95_ms`, `budget_ms` (null when section 9 has none), `pass` (median within budget). 10 runs after one warm-up | M1-01 |
| Nightly benchmark | GitHub Actions on Linux; baseline committed in `bench/baseline-linux.json`, changed only by a pull request | M1-12 |
| History order | Newest first by commit date, never a parent above its child (`git log --date-order`) | M1-02 |
| Whole-repo history | Starts from `HEAD`, local and remote branches; tags only label commits | M1-02 |
| Filtered history | A filtered list has no graph lines | M1-02, M1-08 |
| History without a commit-graph file | Same order, but the first page explores the whole history first, as `git log --date-order` does. With the file, a page reads only a little past its last row. When a first page without the file takes longer than 200 ms, the app offers once per repo to write it with `git commit-graph write --reachable`, as a background job. It never writes the file without asking, never keeps it current, and never changes git config for it | M1-02, M1-08 |
| History matching | Author matches name or email; the hash prefix may be any length; the stash start gives one row with no lines | M1-02 |
| Ref labels | `HEAD` is shown only when detached; the checked-out branch is marked as current; `refs/remotes/*/HEAD` is not shown; short hashes use git's abbreviation length | M1-02, M1-08 |
| Lane layout | Lanes never shift sideways; a freed lane is reused by the next new line. Colors are a running index per line; the UI maps it onto its palette | M1-02, M1-08 |
| History bench | `history-first-page` on `target-project` (200 ms) and `big-history` (300 ms); later pages as `history-page-10` on `target-project` (its last page) and `history-page-100` on `big-history`. Stretch budgets are reported but do not fail `just bench` | M1-02 |
| Date column | Author date in local time, `YYYY-MM-DD HH:MM`; committer date in the tooltip | M1-08 |
| "Uncommitted changes (n)" | n is the number of distinct changed paths: staged, unstaged, untracked or unmerged | M1-08 |
| Paths over IPC | Files cross IPC as a display string plus an id; the UI never sends a path string back (PLAT-2) | M1-03 |
| Image diffs | PNG, JPEG, GIF, WebP and BMP up to 10 MB per side, shown through `blob:` URLs; SVG is diffed as text | M1-03, M1-09 |
| Image detection | A binary side is an image when its first bytes are one of the five formats' signatures; the file name does not matter | M1-03 |
| Changed-file line counts | Counted as `git diff --numstat` does when the list has at most 1,000 files (about 7 µs a file); longer lists have no counts. Binary files and submodules have none | M1-03, M1-09 |
| Renames in diffs | As `diff.renames` says, git's default when unset; `copies` is treated as renames, and a copy is listed as an added file | M1-03 |
| Diff attributes | `binary` and `-diff` are read from the working tree's `.gitattributes`, then the index, as git does; a bare repo uses `HEAD`'s tree | M1-03 |
| Diff bench | `select-commit`: of the first history page of the largest repo, the commit with the most changed lines under 5,000; header, file list and the first file's diff against the first parent, the repo already open. `select-commit-large`: the root commit of the largest repo, which adds 30% of its files. `select-commit` is also reported on `big-history` | M1-03 |
| Working copy lists | Untracked files are in the unstaged list with status `untracked`; a folder git lists as one entry has a path ending in `/` and a `directory` diff. Unmerged paths are a third list, in neither of the others, as in `git status`. The lists have no line counts | M1-04, M1-09 |
| Renames in status | Between `HEAD` and the index as `status.renames`, then `diff.renames`, says; on when unset; copies are treated as renames | M1-04 |
| Status fallback | `git --no-optional-locks status --porcelain=v2 -z` runs instead of `gix` when `core.fsmonitor` is set, the repo is untrusted (`safe.directory`), the index is sparse, the untracked cache is on, or the index has unmerged paths. A file's diff is always read with `gix`: none of these cases changes what a file contains | M1-04 |
| Conflict operation | Read from the git directory in this order: `rebase-merge` or `rebase-apply` is a rebase (`rebase-apply/applying` is `git am`), then `MERGE_HEAD`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`. Unmerged paths are counted from the index | M1-04 |
| Repo summary | The upstream as git shortens it (`origin/main`), marked gone when configured but missing; dirty includes untracked files; a detached `HEAD` has no upstream | M1-04 |
| Repo config | Read when the repo is opened; a refresh reopens the repo to see changes | M1-04, M1-11 |
| Status bench | `status-60k`: the largest `target-project` repo, already open. `status-big-tree`: the bench turns on `core.untrackedCache` (and `core.fsmonitor` on macOS and Windows) in the `big-tree` repo and fills the cache with one `git status` first | M1-04 |
| Text that is not UTF-8 | Decoded lossily for display; no encoding detection | M1-02, M1-03 |
| Tree filter | Done in the core, like history search, so the UI holds no filtering logic | M1-07 |
| Graph drawing | Lanes computed in the core, drawn per row as inline SVG | M1-02, M1-08 |
| Folder picker | Opened from Rust; the front end gets no dialog permission (SEC-2) | M1-05 |
| Repo ids | One more than the highest number in use by a repo or a group, so a removed repo's id is never reused. Removing a repo also takes it out of its groups | M1-05 |
| Same repo | Two paths are one repo when their canonical paths are equal or they are the same folder (device and inode; volume and file index on Windows), which covers case-insensitive file systems | M1-05 |
| Add folder | A folder inside a repo, but not its top, is scanned like any other folder, not resolved up to the repo. A scan that finds nothing reports the folder as "no repo found". Scan progress has a `total` of 0 until the number of repos is known | M1-05 |
| Workspace IPC | `workspace_get` returns the open workspaces in tab order. Adding returns a job id at once; the result arrives as `workspace_changed`, then `workspace_add_report`. `workspace_pick_and_add` returns at once; a cancelled dialog does nothing | M1-05 |
| Workspace file problems | When the newest workspace file is unreadable or has a newer schema, it opens read-only and no other workspace is created beside it | M1-05 |
| Selected repo for refresh | The core keeps each workspace's selected repo, set by `repo_select`; it is not saved, so a start refreshes in workspace order until the UI selects a repo | M1-06 |
| Summary read failures | A repo that exists but cannot be read keeps its cached summary, stays stale and carries the error, git's own message when git refused it; its cache row is left as it was | M1-06 |
| Added repos | The repos an add job adds have their summaries read at once, as at start | M1-06 |
| Summary IPC types | The summary types in `gitcroc-git` derive specta's `Type` directly; specta is not Tauri, so ARCH-5 holds | M1-06 |
| Summary cache | SQLite `user_version` holds the schema; the summary is stored as JSON, and a row this build cannot read counts as missing. The refresh starts once git detection finds a usable git | M1-06 |
| Tree IPC | `tree_get` returns the skeleton: Ungrouped, the repos and their four ref nodes, not loaded. `refs_get` returns one repo's four ref nodes in full, unpaged, when one of them is first expanded; only then are ahead and behind counted (ARCH-8) | M1-07 |
| Tree filter scope | Matches repo names and the full names of local branches, remote branches (with the remote) and tags; stashes are not matched. Refs not loaded yet are read for it, without ahead and behind. A repo matched by name alone keeps its four ref nodes, not loaded. Each request supersedes the previous one of its workspace and returns `null` | M1-07 |
| Refs refresh | A repo's loaded refs are dropped when its summary is read again; `repo_refs_changed` then tells the UI to read them again if it had them | M1-07 |
| Repo row spinner | `SummaryView.refreshing` is set while a summary read waits or runs; `SummaryView.conflict` carries the conflict state so the UI need not derive it | M1-07 |
| History IPC | One walk per workspace: `history_get` returns the first page and a handle, `history_more` the next page of that handle. A newer `history_get` of the workspace ends the previous walk, whose calls then return `null`. Selections take full ref names, so a tag and a branch of the same name stay apart | M1-08 |
| History position | The UI keeps scroll position, selected row, search text and loaded row count per tree selection; going back re-reads as many pages and scrolls back. A repo and the nodes that only group its refs (Local, a folder, a remote) share the repo's history and position | M1-08 |
| Uncommitted count | `RepoSummary.changes` counts distinct staged, unstaged, untracked and unmerged paths from the status the summary read already does. The row shows for a repo or its current branch, not while a search is shown | M1-08 |
| Commit-graph offer | Made when a first page read without a commit-graph file took over 200 ms. "Don't ask again" is kept as an optional `commit_graph = "declined"` on the `[[repo]]`, schema unchanged. The UI shows one offer per repo until answered. The write job reports progress at start and end only, then `commit_graph_written`; a running write is joined, not started twice | M1-08 |
| Timing in CI | Hosted CI runners have no fixed CPU, so tests there assert behaviour, never section 9 times: the history scroll test checks that pages load on demand, all arrive and the DOM stays bounded, and only reports how long rows stayed blank. The 100 ms scroll budget is checked on the reference machine by `just bench-ui` (PERF-8, M1-12). No CI test depends on timing: the watcher tests wait for a specific outcome (the deadline only ends a hung test), and what must not happen and the debounce times are tested without the OS on a simulated clock. Tests that assert times are `#[ignore]`d and run on the reference machine with `just timing`, such as M1-11's 1 s from a commit to its summary | M1-08, M1-11 |
| Diff settings | Mode, options and the file list toggle are global settings; dock position is per workspace | M1-09 |
| Changes IPC | One view per workspace: `changes_get` reads a commit (header and files against one parent) or the uncommitted lists and returns them with a handle; `diff_get` takes the handle and a file's list and id. A newer `changes_get` of the workspace ends the previous view, whose calls then return `null`. The core keeps the lists, so no path crosses back (PLAT-2) | M1-09 |
| Unmerged files in M1 | Listed first in Unstaged and marked with their conflict kind; selecting one shows a note and no diff until the conflict editor (M2) | M1-09 |
| Parent links | Clicking a parent in the commit header selects it; the history scrolls to it only when it is already loaded, and never reads pages to find it | M1-09 |
| Pane layout file | An optional `[layout]` table in the workspace file, schema unchanged: `dock` (`right` or `bottom`), `tree` and `history` widths, `changes` height when docked at the bottom. A missing field takes the UI's default. Saved when a resize ends and when the dock changes | M1-09 |
| Side-by-side context | Side-by-side asks the core for the whole file, whose hunks mark the changed lines (whitespace option applied), and folds unchanged runs beyond 3 or 10 lines in the view; inline mode shows the core's hunks as they are | M1-09 |
| Image bytes over IPC | `diff_image` returns one side of the image diff `diff_get` last read, as base64: Tauri's raw byte responses have no generated type. The UI makes a `blob:` URL from it; the CSP adds only `blob:` to `img-src` | M1-09 |
| Styles under the CSP | CodeMirror's style tag carries the nonce Tauri gives the page's `<style id="csp-nonce">`; `style-src` has no `'unsafe-inline'`. Constant inline styles go in CSS, because Solid turns them into `style` attributes, which the CSP blocks | M1-09 |
| Fonts | Default size 13 px, range 8 to 32 | M1-10 |
| Appearance settings | Top-level keys of `settings.toml` (schema 2; schema 1 migrates with the defaults added), changed together by `settings_set_appearance`. The core trims font names, keeps a blank one as `null` (the system font) and moves sizes outside 8 to 32 to the nearest limit, on a change and on load | M1-10 |
| Code theme colors | Defined in `ui/src/theme/codeThemes.ts` and applied as `--code-*` and `--diff-*` variables on the root, so open editors restyle without rebuilding; the Settings previews set them on themselves. Diff colors are each theme's green, red, blue and yellow mixed into its background. Solarized uses base02 (light) and base2 (dark) for text, as its usual text cannot keep 4.5:1 on any tinted line | M1-10 |
| Watcher debounce | 200 ms per repo after its last event, and at most 1 s after its first while events keep coming; lock files in the git directory alone do not trigger a refresh (in the working tree a `*.lock` file is an ordinary file). The debounce is gitcroc's own: notify's debouncer crates debounce per path | M1-11 |
| What is watched | Per repo: the git directory and, for a linked worktree, the common directory, each without subfolders, and the common directory's `refs/` with them; for the selected repo, its working tree without any `.git` folder. On Linux and the BSDs each working tree folder is its own watch, added by gitcroc so the git directory takes none; on macOS and Windows the tree is one recursive watch and events inside `.git` are dropped | M1-11 |
| History refresh | `SummaryView.refs_version` goes up when the watcher saw `HEAD` or a ref change, or a read in this run finds `HEAD` moved (so a focus refresh catches commits too). The history reads the shown repo again when it goes up, as many rows as were loaded plus one page if the selected commit moved below them, and replaces the rows only when all are read | M1-11 |
| Focus refresh | The UI calls `workspace_refresh` for the open workspace when the window gains focus; the core reads every repo, the selected one first | M1-11 |
| Watch failures | Sent once per workspace as `watch_warning` and kept for `watch_warning`; printed on stderr until the log file (section 10) exists. A repo whose folder is missing is not watched and gives no warning | M1-11 |
| Regression gate | Only `target-project` scenarios with a section 9 budget are gated, as PERF-8 says "budget". They fail when over the budget, or when more than 20% and also at least 2 ms slower than the baseline: hosted runners vary by more than 20% on scenarios of a few milliseconds. Other slowdowns are reported as `slow`. A scenario missing from the baseline is reported as new and passes, so it can land before its baseline does. `gitcroc-cli bench-compare` judges, `gitcroc-cli bench-baseline` takes the median of three runs' medians | M1-12 |
| Bench workflow runs | Nightly at 03:00 UTC and on demand, on `ubuntu-22.04`. Without a committed baseline, or when dispatched with `baseline`, it makes one instead of comparing. A pull request that changes the workflow runs it but only reports a regression, so no pull request check depends on timing | M1-12 |
| UI measurement | An action is timed from its first `pointerdown`, `mousedown` or `keydown` to the frame after its result is in the DOM. Cold start runs from the start of `run()` to the frame after the tree shows a repo's cached branch. Scroll: 100 px a frame through the largest repo's history; 60 fps means a median frame of at most 16.7 ms. Switching alternates the two largest repos and ends when the history's top row changes. Input latency is a key in the tree filter while the focus refresh of every repo runs. Memory is the proportional set size of the app and its WebKit processes after 3 s with no refresh. Five runs after one warm-up | M1-12 |
| UI bench driver | `ui/bench/bench-ui.ts`, run by Node without a build step, speaks W3C WebDriver to `tauri-driver` over HTTP with no packages | M1-12 |
| Writes in M1 | `git commit-graph write`, offered by M1-08, is the one git write in M1: it writes a cache file in the git directory and changes no ref, object, index or working file. The read-only test allows it by name | M1-08, M1-12 |
| Libraries | `gix`, `rusqlite` with bundled SQLite, `notify` with a debouncer, CodeMirror 6 with `@codemirror/merge`, TanStack Virtual; versions recorded in each pull request | M1-02 to M1-11 |
| What M2 completes | Single-repo forms of UI-7 (without dragging repos between groups), UI-13, UI-15 and UI-20; UI-21 in full. GRP-7's outcomes without *skipped*, and GRP-8, GRP-9, GRP-10 and GRP-12 apply to single-repo jobs and are completed for groups in M3. The command palette (UI-22) is M5. Group commit (GRP-15, UI-34) is M3 | M2-01 to M2-14 |
| Where writes live | `gitcroc-git::ops`, one function per operation; `gitcroc-core` runs them as jobs. `arch_1_writes_go_through_the_runner` replaces M1's read-only test: outside `ops`, runner calls are read-only | M2-01 |
| Write environment | Writes run with `GIT_EDITOR=true` besides `LC_ALL=C` and `GIT_TERMINAL_PROMPT=0`, so git never waits on an editor; messages are passed explicitly, and Continue keeps git's prepared message | M2-01, M2-05, M2-08 |
| Job pool | One pool of 8 workers runs jobs and the background refresh; user jobs before the refresh, which keeps its PERF-4 order and lowest priority. The 1 to 32 setting of section 5 arrives with group operations in M3 | M2-01 |
| Busy repo | Writes on one repo queue in the order they were started (ARCH-4). `SummaryView.busy` is set while one waits or runs; the UI disables operations for a busy repo (GRP-8), but staging controls stay enabled and queue | M2-01, M2-04 |
| Background refresh reads | One repo at a time per workspace, each read holding a worker of the shared pool. M1-06 asked for one per core up to 8, but from M1-13 on the refresh in fact read one at a time, and M1's budgets were met that way. Reading up to 8 at once took the refresh of `target-project` from 0.9 to 0.33 s, but `memory-idle` to 460 MB and cold start to 1112 ms, over budget (Eric, 2026-10-13) | M2-01 |
| Outcome rules | *Nothing to do* is decided from refs and `HEAD` before and after, never from git's wording; *conflict* from the repo's conflict state after the run (OPS-1) | M2-01 |
| Operations over IPC | One command per area (for example `op_branch`, `op_sync`) taking a generated tagged enum of that area's operations and returning a job id at once; the outcome arrives as `job_ended` | M2-01 to M2-14 |
| Destructive operations | The core decides which requests need confirmation and refuses them without `confirmed: true`; the UI only shows the dialog. GRP-9's list (discard, force delete, remote branch delete, hard reset, force-with-lease push), plus four that also cannot be undone from the app: stash drop, stash apply abort, remote remove, tag delete on the remote | M2-04, M2-06, M2-07, M2-11, M2-12, M2-13 |
| Job list | Lists jobs the user started and the commit-graph write, not background refresh reads; kept in memory until the app closes. A *failed* or *conflict* outcome stays in the status bar until the job list is opened | M2-02 |
| `gitcroc.log` | Every git process, read or write: arguments, working directory, duration, exit code; never stdin, stdout or file content. Watcher warnings go there instead of stderr | M2-01 |
| Askpass helper | The gitcroc executable started by git with `GITCROC_ASKPASS` set; it relays git's prompt to the running app over a local socket with a per-job token, and the app asks in a modal dialog of its own window. Section 4 says "a native dialog"; this reads it as the app's own dialog, since the operating systems have no common native text-input dialog. `SSH_ASKPASS_REQUIRE=prefer`. Set on network writes only | M2-03 |
| Commit message | Subject, blank line, body, passed with `git commit -F -` on stdin; the unsent message is kept per workspace and repo while the app runs. Amend fills an empty box with the last message | M2-05 |
| Remote branch checkout | Switches to a local branch of the same name when one exists, otherwise creates a tracking branch; never resets an existing local branch | M2-06 |
| Push target | The upstream; without one, `origin`, or the only remote when there is no `origin`; with several remotes and no `origin` the dialog asks. Tag push the same way | M2-07, M2-12 |
| Cherry-pick and revert | A multi-selection is cherry-picked oldest first in one `git cherry-pick`. Reverting a merge commit asks for the mainline parent, the first by default | M2-08 |
| Conflict merge | Hunks from `gix`'s blob merge of stages 1 to 3 in diff3 style, checked against `git merge-file` in tests. A file is a choice conflict (OPS-6) when it is binary, not UTF-8, over 1 MB on a side, or the stages are not two regular files of the same mode | M2-09 |
| Edited outside | The file on disk is compared with the stages rendered in the repo's `merge.conflictStyle`, ignoring the label text after each marker; any other difference (an edit, rerere, a merge driver) makes the editor ask. "Keep the file as it is" opens it as plain editable text without hunk buttons | M2-09, M2-10 |
| Conflict markers | A marker line starts with exactly seven `<`, `=`, `\|` or `>` followed by a space or the line end; Mark resolved refuses while one remains, unless overridden | M2-09 |
| Mark resolved write | The core writes the text keeping the file's mode and its line endings (CRLF when the file on disk used CRLF), then runs `git add` | M2-09 |
| Stash apply conflicts | Named "stash apply" from the unmerged entries alone. Abort runs `git reset --merge` after a confirmation; Continue is never needed, because the conflict state ends with the last Mark resolved; after a pop, the app offers to drop the stash git kept | M2-11 |
| fsmonitor offer | Shown in the changes pane; "don't ask again" kept as `fsmonitor = "declined"` on the `[[repo]]`, schema unchanged. On Linux only `core.untrackedCache` is offered | M2-13 |
| Clone | Into a parent folder and a name defaulting to the URL's last segment; a non-empty target is refused; cancel and failure remove the folder the clone created. Added to the open workspace and selected | M2-14 |
| Terminal | Linux: `x-terminal-emulator`, then `gnome-terminal`, `konsole`, `xterm`; macOS: Terminal; Windows: Windows Terminal, else `cmd` | M2-14 |
| End-to-end smoke | Linux only for now, locally and in CI's Linux job; Windows deferred like the other Windows runs. It waits for results in the DOM, never for a time | M2-15 |
| What M3 completes | UI-27 to UI-34, GRP-2 to GRP-15, PERF-5, DATA-2, and the group forms of GRP-7 (*skipped*), GRP-8, GRP-9, GRP-10 and GRP-12. UI-1, UI-2 and UI-7 are completed with groups and dragging. The command palette (UI-22) stays M5, but every new control is reachable by keyboard | M3-01 to M3-14 |
| Workspace names | Trimmed, 1 to 64 characters, unique among the workspace files ignoring case, as GRP-2 has for groups | M3-01 |
| Rename and delete a workspace | Rename from the tab's context menu. Delete from the "+" menu, only for a workspace that is not open, after a confirmation. It removes the workspace file and nothing else | M3-01, M3-02 |
| Session file | An optional `[session]` table in `settings.toml`, schema unchanged: `tabs` (open workspace ids in order) and `active`. Without it, the M1 rule opens the newest file or creates "My workspace". A missing file is left out with a warning | M3-01 |
| No tab open | Closing the last tab is allowed; the window shows an empty state with the "+" menu's actions | M3-01, M3-02 |
| Closing a tab | Running jobs keep running and stay in the job list. Unsaved conflict editor state (OPS-9) asks first | M3-02 |
| Tab keyboard | Ctrl/Cmd+Shift+PageUp/PageDown moves a tab | M3-02 |
| Unseen failure | A *failed* or *conflict* outcome is seen when the job list is opened or its report is shown in the active tab (M2-02's rule, per tab) | M3-03 |
| Group ids | `g1`, `g2`, … in file order at load, kept in memory only, so the section 10 format is unchanged and a rename keeps the id for the session | M3-04 |
| Group name "Ungrouped" | Refused, ignoring case, so a group cannot be confused with the built-in node | M3-04 |
| Mixed count | Distinct current branches of the members whose summary is known; every detached member counts together as one value "detached"; missing members do not count | M3-04 |
| Group filter | The tree filter also matches group names; a group matched by name shows all its members | M3-04 |
| Dragging | Onto another group moves the repo from the group it was dragged from; Ctrl (Alt on macOS) adds instead; from Ungrouped always adds; onto Ungrouped removes it from its group. Every drag also exists as a context menu item | M3-05 |
| Multi-selection order | A multi-selection's target set is in tree order without duplicates. Shift-click ranges over visible repo rows only. Clicking a non-repo row replaces the multi-selection | M3-06 |
| Merged history order | Newest first by commit date; ties by target-set order, then each repo walk's order. A commit in two repos shows once per repo. Unborn and missing repos add no rows | M3-07 |
| Merged list menu | The commit menu acts on the row's repo; cherry-pick takes a multi-selection only from one repo | M3-07 |
| Preflight over IPC | Returns an id at once; the result arrives as `preflight_ready`. Not listed in the job list. Used once; refused when the workspace changed since it ran | M3-08 |
| Who runs | *Will run* and *needs attention* repos run. *Will skip* and *blocked* repos are reported as *skipped* with their reason, so GRP-7's five outcomes stay exact. GRP-8 is checked again when each sub-job starts | M3-08 |
| No unchecking in preflight | The dialog cannot remove repos from the target set, since GRP-5 allows no other way to build one; the user changes the selection instead | M3-09 |
| Parallelism setting | `group_parallelism` in `settings.toml`, 1 to 32, default 8. It sizes the shared pool (PERF-4), so single jobs and the background refresh share it | M3-08 |
| Report opens by itself | When a group job ends with a *failed* or *conflict* outcome and its tab is active; otherwise the tab marker shows | M3-09 |
| Askpass in group jobs | Prompts from parallel sub-jobs queue in the one dialog, each naming its repo; cancelling one fails only that sub-job | M3-10 |
| Group push without origin | Section 5 names `origin/<branch>`, so a repo with no upstream and no `origin` is *will skip*; the single-repo rule of the only remote does not apply to groups | M3-10 |
| Checkout policy classes | "Switch to default branch" and "create X from HEAD" make the repo *needs attention* with what will happen. X on several remotes and not local is *will skip* | M3-11 |
| Group remote branch delete | On the remote of X's upstream, else `origin`; with neither, the local delete runs alone and the result says so | M3-11 |
| Conflict prediction | `git merge-tree --write-tree HEAD X` in preflight. It writes objects only, so the runner allows it as read-only, like M1's commit-graph write. For rebase the same check is labelled "may conflict" | M3-12 |
| Group commit options | "Stage all tracked changes first" runs `git add -u` in the sub-job. "Push after commit" pushes in the same sub-job; a failed push after a made commit is *failed* with a message saying so. The confirmation always shows | M3-13 |
| Group message box | Kept per workspace and target set while the app runs; cleared when every repo that ran committed | M3-13 |
| Export content | Name, repos and groups with paths relative to the exported file and `/` separators. No layout and no offer answers. A path on another Windows drive stays absolute | M3-14 |
| Import | Makes a new workspace (name gets " (2)" when taken) with the file's ids and groups. Missing repos stay with the missing-path warning, and the clone dialog fills in their path, keeping their id | M3-14 |
| Tab-switch timing | From the click to the frame after the history's top row changes, between two workspaces with their largest repo selected | M3-15 |
| What M4 completes | AI-1 to AI-15, D9, D11, SEC-1 and SEC-5, and D4 in full. SEC-3 holds for proposals as for any repo text; SEC-7 and D12's update check wait for M5 | M4-01 to M4-12 |
| Core and AI seam | `gitcroc-core` may not depend on `gitcroc-ai` (section 4 "Crates"), so the core defines its own context and proposal types and an async `Proposer` trait. `gitcroc-app` and `gitcroc-cli` each implement it with a newtype over `gitcroc-ai` that maps fields only | M4-07, M4-08 |
| Reply parsing | One JSON object after trimming and at most one Markdown code fence; unknown keys refused. Transport and process errors fail the file at once; only invalid replies are retried (AI-9) | M4-01 |
| Data delimiters | Repo text between `<<<DATA token …>>>` and `<<<END token>>>` lines, with a random token per request that the repo text cannot contain (SEC-5) | M4-02 |
| Split failure | When one part of a split file fails, the whole file is *AI failed* and no partial proposal is kept | M4-02 |
| Request timeout | 5 minutes per request, not a setting | M4-03 |
| Temperature refused | An OpenAI-compatible endpoint that refuses `temperature` gets the request again without it, remembered for the session | M4-03 |
| CLI prompt | System prompt and user message as one text on stdin; no temperature, since neither CLI takes one. Flags recorded in the code with the CLI version they were checked on | M4-04 |
| AI settings | An optional `[ai]` table in `settings.toml`, schema unchanged: `default`, `exclude`, `debug_log`, and `[[ai.provider]]` entries with `id` (`p1`, …), `name`, `kind`, `model`, `base_url`, `path`, `context_budget` and `consent` | M4-05 |
| Keychain entries | Service `gitcroc`, account `ai-provider-<id>`. An unreachable keychain is reported, never replaced by a file. Tests and the `e2e` build use an in-memory keychain | M4-05 |
| Keys in the CLI | `gitcroc-cli` reads a key only from an environment variable named by `--key-env`, never from an argument or the keychain | M4-05, M4-11 |
| Consent record | Stored per provider as the host (API) or program path (CLI) consented to; asked again when that changes | M4-06 |
| Workspace AI switch | An optional `ai = false` in the workspace file, schema unchanged, set from the tab menu; not exported | M4-06 |
| Exclude patterns | Globs matched against the file name and the path from the repo top; case-insensitive on Windows and macOS | M4-07 |
| Hunk ids | Numbered from 1 in file order over the core's chunks; only hunks still untouched in the editor are sent | M4-07, M4-09 |
| Context lines | The 40 lines before and after a hunk come from the merged text around it: resolved text and the ours side of neighbouring hunks | M4-07 |
| Side commits | `HEAD` and `MERGE_HEAD`, `CHERRY_PICK_HEAD` or `REVERT_HEAD`; for a rebase the commit being applied; for a stash apply the stash commit. Messages are those of commits touching the file, reachable from the side and not from the two sides' merge base | M4-07 |
| AI jobs | In the job list as "Resolve with AI · <file>" with outcome and token usage, never the prompt. They do not take git pool workers; at most 4 AI requests run at once | M4-08, M4-10 |
| Proposal lifetime | In core memory per workspace, keyed by repo, path and stage ids; dropped on Abort, Continue, Mark resolved, a side chosen, or changed stages | M4-08 |
| Edit | Opens the proposed text in a small inline editor; its Accept puts the edited text in the buffer | M4-09 |
| Stale proposal | A hunk edited after its proposal arrived shows "hunk changed" with Accept disabled, the take buttons' rule | M4-09 |
| Fixture format | One folder per fixture with `meta.toml`, `base`, `ours`, `theirs` and `accept/*.txt` (whole-file resolutions, or `UNRESOLVED`); written for this repo and reviewed by Eric | M4-11 |
| Eval score | *Match* when the resolved file equals an accepted resolution after normalizing line endings and trimming trailing whitespace | M4-11 |
| AI in the smoke test | A mock provider kind in the `e2e` build, chosen by `GITCROC_E2E_AI` | M4-12 |
| What M5 completes | UI-22, PLAT-4, SEC-7, D12 and D15 in full; tests for D1, D2, D3, D5, D7, D8 and D13; `just coverage spec` empty, with DEV-1 to DEV-4 as no test needed (ADR 0003) | M5-01 to M5-20 |
| Windows and macOS proof | Installing and updating on Windows and macOS is proven by CI jobs on GitHub's hosted runners (`windows-latest`, `macos-latest` on Apple silicon), as is the smoke test on Windows and the OS keychain on both. Budgets, screen readers and by-hand checks there stay deferred (Eric, 2026-10-19) | M5-14, M5-15, M5-16, M5-20 |
| D15 in the core | D15's "all strings externalized" is met by one strings module for the UI. Text the core sends (errors, skip and block reasons, palette labels, git's output) stays English built in Rust, one place per type, and is shown as it arrives; translating it is LATER with translations (section 2) | M5-04 to M5-07 |
| Strings module | `ui/src/strings/en.ts`, one `const` object by area; a text with values is a function, which also holds the plural rule. No translation library | M5-04 |
| Palette matching | Case-insensitive; every word of the query must occur, in any order. Ranked: whole name, name prefix, word prefix, anywhere; ties by kind (operation, repo, group, branch), then catalog or tree order. At most 100 items with the total; an empty query lists the operations. Branches are local and remote, not tags or stashes | M5-07 |
| Palette contents | The catalog of operations lives in the core with its labels; the UI only maps each id to the handler its toolbar, menu or pane entry uses, and disables entries by the same rules. AI entries are absent while AI is off (AI-16) | M5-07, M5-08 |
| Keyboard shortcuts | Ctrl/Cmd+K palette; F6 and Shift+F6 between regions; Ctrl/Cmd+, Settings; Ctrl/Cmd+PageDown and PageUp between tabs; F7 and Shift+F7 between hunks. All in `ui/src/keys.ts`; none shadows a CodeMirror key | M5-09 |
| Accessibility target | WCAG 2.2 A and AA as axe-core checks them, in both app themes; app theme text 4.5:1 and focus, borders and selection 3:1; reduced motion and forced colors respected; a by-hand Orca pass on Linux | M5-10, M5-20 |
| Settings keys in M5 | `git_path` (optional OS path) and `update_check` (default on), optional top-level keys of `settings.toml`, schema unchanged | M5-11, M5-12 |
| Missing chosen git | A chosen git path with no executable falls through to M0-06's search, and Settings says so; a chosen git that exists but is too old is reported | M5-11 |
| Update check | Release bundles only, never debug, `e2e` or `perf-trace` builds. First after the active workspace's startup refresh, then every 24 hours while running; nothing about checks is saved. A failed check shows only in Settings and the log | M5-12 |
| Updates by package | Windows installer, macOS app and AppImage install and restart; `.deb`, `.rpm` and the Windows portable zip only notify and open the fixed releases page. Installing is a job, refused while another job runs; unsaved conflict editor state asks first | M5-12 |
| Update endpoint | `https://github.com/ericlass/gitcroc/releases/latest/download/latest.json`; pre-releases are not offered, since `releases/latest` ignores them | M5-12, M5-13 |
| Updater key | Eric generates the key pair and stores the private key and its password as repository secrets; agents only ever see the public key. CI update tests use a throwaway key pair made inside the job | M5-12, M5-13, M5-15 |
| Releases | A pushed tag `v<version>` builds a draft GitHub release with every package, `.sig` files, `SHA256SUMS` and `latest.json`; notes from `CHANGELOG.md`. Eric pushes tags and publishes; an agent never does | M5-13 |
| Release candidate | `1.0.0-rc.1` before `1.0.0`, published as a pre-release; if a package format orders it wrongly, the scheme is asked about, not invented | M5-20 |
| Title bar setting | `system_title_bar` (default off), a top-level key of `settings.toml` beside the fonts, schema unchanged | M5-23 |
| History columns | Every column can be moved; order and widths per workspace in `[layout.columns]` of the workspace file, read leniently, schema unchanged; Move left, Move right and Reset columns in a header menu | M5-24 |
| Start line in the log | `gitcroc <version> started (<os> <arch>, <package>)` at each start, no paths; the install and update tests wait for it | M5-14 |
| Website and screenshots | No website in v1: the README is the front page and GitHub Releases the download page. No screenshots | M5-18, M5-19 |
| User docs | Markdown in `docs/user/`; `docs/install-unsigned.md` moves to `docs/user/install.md` | M5-18 |
