# Contributing to gitcroc

gitcroc is a Rust Cargo workspace. Crates live in `crates/`; `crates/gitcroc-app` is the app's API with no UI toolkit in it. Milestone M6 rewrites the UI in GPUI as `crates/gitcroc-ui` ([the plan](docs/plans/native-ui-gpui.md)). The Tauri app was removed in M6-05: 1.0.0-rc.1 was its last release, and until M6-06 there is no app to run. The old SolidJS front end stays in `ui/` as the reference the M6 tasks port from, outside `just check` and CI, until M6-30 deletes it.

## Setup

You need:

- the Rust toolchain pinned in `rust-toolchain.toml`, `just`, and `cargo-deny`;
- git 2.40 or newer (D13). The app and the tests use the system git; `cargo run -p gitcroc-cli -- git-info` prints which one is found;
- only for `just ui-reference`: Node (the version in `.nvmrc`) and pnpm (the version in `ui/package.json`'s `packageManager` field; `corepack enable pnpm` installs it).

`just ui-reference` runs the old front end in a browser with its fake core (`ui/src/ipc/fake.ts`), served by Vite.

## Running checks

Run `just check` before you call a change done. It runs everything CI requires:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo deny check`
- `just coverage M0` to `M5` and `just coverage spec` (the requirement coverage report; it fails if a requirement of that milestone has no test). Until M6-06 it still counts the tests under `ui/`, which no longer run.

Run `just fmt` to format the workspace.

## The old front end

`ui/` holds the SolidJS front end of 1.0.0-rc.1. Its calls went through `ui/src/ipc/bindings.ts`, which tauri-specta generated from the Tauri commands; the file is no longer regenerated. Nothing in `ui/` is built or tested by `just check` or CI. The M6 tasks port its views, stores and tests to `crates/gitcroc-ui`, and M6-30 deletes it.

`just coverage <milestone>` (for example `just coverage M1`) lists requirement IDs from the `Implements:` lines of `docs/tasks/<milestone>-*.md` that have no test, and fails if there are any.

## Writes, jobs and askpass (M2)

**Where writes go (ARCH-1).** Every git command that changes a repo or contacts a remote is a function in `gitcroc-git::ops` (`crates/gitcroc-git/src/ops/`). Each one builds its argument array (ARCH-3: paths after `--` or as a pathspec file on stdin, never a shell string) and runs it through `GitRunner::run_write`, which returns git's output or a `GitError` carrying git's stderr. Nothing else starts git for a write: `arch_1_writes_go_through_the_runner` (`crates/gitcroc-git/tests/writes.rs`) reads the shipped crates' source and fails on a write outside `ops` or a process started outside the runner. The one other file that starts a program is `crates/gitcroc-app/src/platform/launch.rs`, for the file manager and a terminal.

A new operation also needs a row in `crates/gitcroc-git/tests/ops_matrix.txt` with its tests for success, failure and, where git can stop in one, conflict. `ops_matrix_every_operation_has_success_failure_and_conflict_tests` fails on a `pub async fn` of `ops` that is in no row, and on a test name that does not exist.

**The job engine (ARCH-7, `crates/gitcroc-core/src/job/`).** An operation in `gitcroc-core` checks its request (refusing what git would refuse, before git runs), then starts a `WriteJob` with `Jobs::start` and returns the job's id at once. The job:

- waits in the queue of each repo it writes to, so at most one write runs per repo (ARCH-4); reads never wait for it;
- runs on the shared worker pool, before the background refresh (PERF-4);
- reports progress, which the app sends to the UI as events;
- ends with one outcome of GRP-7: *succeeded*, *nothing to do* (decided from the refs and `HEAD` before and after, never from git's wording), *skipped*, *failed* with git's output, or *conflict* when git left the repo in conflict state;
- has the repos it touched read again when it ends (OPS-7).

Cancel (GRP-10) removes a queued job, and kills a running fetch or clone (`OnCancel::Kill`); other writes run to their end. Work that is not a repo write, such as adding repos or a clone that has no repo yet, is tracked with `Jobs::track` or `Jobs::track_network`. Tests wait for a job's end event, never for a time.

**The askpass helper (D10, `crates/gitcroc-app/src/askpass.rs`).** A network write (`WriteJob::network`) gets `GIT_ASKPASS` and `SSH_ASKPASS` pointing at the gitcroc executable itself, with an address and a token for that job. Started by git in that mode, the executable opens no window: it sends git's prompt to the running app over a local socket, prints the answer and exits. The app asks the user in a dialog of its window. The answer goes to git and nowhere else (never stored, logged or cached), and the token is revoked when the job ends.

## Groups and group operations (M3)

**Groups and target sets.** A workspace's groups live in its `workspace.toml` (section 10) and change only through `Workspaces` (`crates/gitcroc-core/src/workspace/group.rs`): names are unique per workspace ignoring case (GRP-2), and removing a repo from a group never touches the disk (GRP-3). Anything that acts on several repos takes a `TargetSet` (`crates/gitcroc-core/src/target.rs`, GRP-5): one repo, a group's members in group order, or a multi-selection of repos in tree order. `target_repos` resolves it against the workspace as it is now, so a group deleted meanwhile is an error, never a stale list. The UI sends the target set it shows (`TreeStore::target`) and never builds a list of repos itself.

**The engine (section 5, `crates/gitcroc-core/src/group/`).** `GroupOps::preflight` reads every repo of the target set in parallel on the shared pool, read-only, and classes it *will run*, *will skip*, *needs attention* or *blocked*. GRP-8 blocks a missing repo, one in conflict state and one busy with another write before the operation is asked. The result goes out as `preflight_ready`, with whether the user must confirm. `GroupOps::start` then runs one job with one sub-job per repo that runs, at most the group parallelism at once. It ends with a result report: one line per repo of the target set, each with one outcome of GRP-7. Nothing is ever stashed (GRP-12), and the repos that ran are read again.

**Adding a group operation.**

1. Add a variant to `GroupOp` (`group/op.rs`) with the options the user picks, and map it in `GroupOp::operation`. Check names there (SEC-4), so that a bad one is refused before any preflight runs.
2. Implement `GroupOperation` for it. `classify` decides one repo's class with the read API or git's reads, never a write: say *will skip* or *needs attention* with a reason the user can act on. `spec` says how a sub-job runs: whether cancel kills it, whether it may end in conflict, how *nothing to do* is decided, whether it needs the network (and so askpass), and whether it may run in conflict state. `destructive` lists what cannot be undone; when it is not empty the preflight always asks (GRP-9). `run` does the write through `gitcroc-git::ops` with `JobRun::write` (ARCH-1), like the single-repo operation it mirrors.
3. Test it with real git on fixture repos, through `gitcroc-cli group run <group> <op>` (`crates/gitcroc-cli/tests/integration/group_*.rs`): one repo of each class, and the report's outcome and reason for each. Section 12's group scenarios, such as `grp_mixed_branch_scenario`, are asserted on that report.
4. Wire the UI: an entry in the group row's menu or a dialog that runs it with the target set. The preflight dialog and the report come with it. This step is written again once `gitcroc-ui` has the tree (M6).

## AI conflict resolution (M4)

**How the parts fit together.** AI only proposes (D9): a proposal lands in the conflict editor's buffer when the user accepts it, and only **Mark resolved** and **Continue** write anything.

- **The context, in the core** (`crates/gitcroc-core/src/ai/`). `gitcroc-core` decides whether a file may go to AI (AI-4) and builds exactly what is sent for it (AI-5), an `AiFileContext`. Both are read from the conflict the editor reads (OPS-8), never from the markers on disk. The proposal jobs (`proposal.rs`) and **Resolve all with AI** (`all.rs`) run on the job engine, can be cancelled and drop their proposals when AI is turned off (AI-13, AI-16).
- **The `Proposer` seam.** The core does not depend on `gitcroc-ai`. It asks a `Proposer` (`proposal.rs`) for a file's proposal, and a `ProviderAccess` for what it cannot check itself: self-tests, consent and keys. The app implements both in `crates/gitcroc-app/src/ai.rs` and `proposer.rs`, and the CLI does too in `crates/gitcroc-cli/src/ai_resolve.rs` and `proposer.rs`. Each maps `AiFileContext` field by field onto `gitcroc-ai`'s `FileRequest`.
- **The ask loop, in `gitcroc-ai`.** `resolve_file` (`resolve.rs`) splits a file's hunks over the provider's context budget, and a hunk too large on its own comes back unresolved (AI-10). `ask` (`ask.rs`) builds the prompt from the current prompt version (`prompt.rs`), sends it, validates the reply (`validate.rs`, AI-8, AI-9) and retries once with the validation error.
- **The providers.** A provider implements `Provider::complete`: one request, one whole reply, with token usage when the provider reports it (AI-14). `connect` (`connect.rs`) builds one from a `ProviderSpec`: the API kinds over HTTP (`anthropic.rs`, `openai.rs`) and the CLI kinds as a process in an empty temporary folder with tools off (`cli.rs`, `claude_code.rs`, `codex.rs`, AI-2). Keys exist only in the keychain and in the `ApiKey` handed to a provider. They never reach a log, an error or the UI (SEC-1).

**Adding a provider kind.**

1. Add a variant to `ProviderKind` (`crates/gitcroc-core/src/settings/ai.rs`) with what it needs and refuses in `ProviderEntry::check` and `ProviderDraft::check_connection`. Say whether it `takes_key` or `is_cli`, and say in `consent_target` (`ai/proposal.rs`) where it sends (AI-6).
2. In `gitcroc-ai`, add the provider and its `ProviderSpec` variant, and build it in `connect`. Test it against the fake HTTP server (`crates/gitcroc-ai/tests/integration/fake_server.rs`) or `fake-cli` (`tests/support/fake_cli.rs`), never a real provider.
3. Map it in the app's `spec` and `ProviderCalls` (`crates/gitcroc-app/src/ai.rs`): its models (AI-1) and, for a CLI, detection and the self-test (AI-2). Map it in the CLI's `spec` (`crates/gitcroc-cli/src/ai_resolve.rs`) and the eval's `--kind` names (`crates/gitcroc-cli/src/eval/mod.rs`).
4. Add its label to the provider form's kind labels. The kinds Settings offers come from the core's view. This step is written again once `gitcroc-ui` has the settings (M6).

The `mock` kind exists only for the smoke test: only the `e2e` build offers and connects it, and every other build refuses it.

**Adding a prompt version (AI-15).** Prompts live in `crates/gitcroc-ai/prompts/<version>/` as `system.md` and `user.md`.

1. Copy the current folder to the next name (after `v2`, `v3`) and change it.
2. List it in `PROMPT_VERSIONS` (`crates/gitcroc-ai/src/prompt.rs`); `ai_15_every_prompt_folder_is_a_listed_version` fails on a folder that is not listed.
3. Compare it with the current version on the fixture set: `just ai-eval --compare v2,v3` with a provider, for example `--provider p1 --key-env ANTHROPIC_API_KEY` or `--kind claude_code --path "$(which claude)"`. It scores both versions on every fixture (`crates/gitcroc-ai/fixtures/`, format in its `README.md`) and lists the fixtures whose score changed. The key is read only from the variable `--key-env` names.
4. To ship it, make it `CURRENT` and give it a golden prompt next to the others in `tests/prompts/`, and keep the old version's golden test. That is a pull request of its own, with both runs' summaries.

**No test calls a real provider.** The tests use `MockProvider` (the `mock` feature of `gitcroc-ai`), the fake HTTP server, `fake-cli`, or the CLI's `--mock` replies file. They wait on gates and events, never on a time. `just ai-eval` is the only thing that calls a real provider, and it is run by hand, never in `just check` or CI.

## End-to-end smoke test

Section 12's smoke test drove the Tauri app through WebDriver and left with it. M6-29 brings it back as a scenario runner inside the GPUI binary, behind the `e2e` feature.

CI also runs the ignored `d11_os_keychain_round_trip` test on Windows and macOS:
`cargo test -p gitcroc-app --test integration keychain::d11_os_keychain_round_trip -- --ignored --exact`.
It writes generated test data under a fresh `gitcroc-test-<uuid>` service, reads it
back, deletes it and verifies it is gone. It never accesses gitcroc credentials.
Linux needs an unlocked Secret Service and remains a local check.

## Colors and themes (UI-23)

Colors exist only as CSS variables in `ui/src/theme/tokens.css`, defined once for `:root[data-theme="light"]` and once for `:root[data-theme="dark"]`. Stylesheets and inline styles use `var(--…)`; the test `UI-23 no hardcoded colors` (`ui/src/theme/colors.test.ts`) fails on a hex value, a color function such as `rgb(` or a named color anywhere else.

The app's settings live in `settings.toml` in the platform config dir under `gitcroc/` (for example `~/.config/gitcroc/settings.toml` on Linux).

## UI text (D15)

All text the UI shows comes from `ui/src/strings/en.ts`, one exported `strings` object grouped by area (`tree`, `history`, `integrate`, …). A component imports it and writes `strings.clone.title`, never the text itself. UI text is JSX text with a letter in it, and string literals that reach the user through `title`, `aria-label`, `aria-description`, `placeholder`, `alt`, `label`, a `Show` or `Switch` `fallback`, or the dialog parts' text props (`ui/src/dialogs/parts.tsx`). Text that is data is not UI text: names, paths, refs, git's output, and every string the core sends. The test `d15_ui_text_comes_from_the_strings_module` (`ui/src/strings/strings.test.ts`) reads every UI source file and fails on inline text; text that must stay inline, such as the sample code of a code theme preview, is listed in its `ALLOWED` with the reason.

A text with values is a function of them, with the values as named parameters: `` clonesInto: (path: string) => `Clones into ${path}` ``. A function also holds the plural rule, with the `plural` helper in `en.ts`: `stagedFiles: (count: number) => plural(count, "staged file", "staged files")`.

The core's text (error messages, skip and block reasons, labels the core builds) is English, built in Rust, one place per type: the `Display` of the error or reason type. The UI shows it as it arrives. Translating either side is LATER (section 2).

## Continuous integration

CI (`.github/workflows/ci.yml`) runs `just check` on Linux (ubuntu-22.04), Windows and macOS for every pull request and every push to `main`. It calls only `just check`, so local and CI checks cannot drift (QA-5). The repository owner must mark the three CI jobs (`check (ubuntu-22.04)`, `check (windows-latest)`, `check (macos-latest)`) as required status checks in the branch protection rule for `main`; this is a manual step in the GitHub settings.

On Windows, `just` needs `sh` on PATH, which Git for Windows provides.

## Builds

There are no packages and no releases from `main` until M6-28 packages the GPUI app. The Tauri bundles, the install tests and the update test left with Tauri in M6-05; M6-28 writes their replacements, starting from `.github/workflows/` and `.github/scripts/` as they were before M6-05. A fix for 1.0.0-rc.1 would be built from a branch off the `v1.0.0-rc.1` tag, whose workflows are still there.

The version number lives only in the workspace `Cargo.toml`. Only release builds check for updates; debug and `e2e` builds never do (D12).

## Releasing

[RELEASING.md](RELEASING.md) has the steps of a release.

## Test naming (QA-3)

A test for requirement X carries X in its name or in a comment. For example, `grp_6_one_failure_does_not_stop_others` tests GRP-6. The coverage script accepts both `GRP-6` and `grp_6`. It searches test code under `crates/` and `ui/`, which means:

- Rust files under a `tests/` directory
- Rust code after a `#[cfg(test)]` attribute
- Front-end files named `*.test.*` or `*.spec.*`, or files under `tests/`, `e2e/` or `__tests__/`

A task file may list an ID on a `No test needed:` line with a reason in parentheses, for example `No test needed: QA-1 (the workflow itself is the proof)`.

## Fixture repos (QA-2)

Build fixture git repositories in code, with helper functions. Never check in repo archives (zip or tar) or other binary fixtures.

The helpers are in `tools/repo-gen` (`repo_gen::Fixture` and `repo_gen::Repo`): a repo in a temporary directory, files, commits, branches, tags, merges, conflicts, stashes, and remotes as a second local repo. Add `repo-gen.workspace = true` to a crate's `[dev-dependencies]`; a test fails if a crate uses it as a normal dependency. The helpers run `git` with argument arrays and hide the user's and the system's git config, so commit ids are the same on every machine.

## Benchmarks (PERF-8)

`just bench-data <set>` builds a reference data set of spec section 9 in `target/bench-data/<set>`, or in `$GITCROC_BENCH_DATA/<set>` when that is set. The generated sets are written by `tools/repo-gen` through `git fast-import`, with a fixed seed, so every machine gets the same commits. A set that is up to date is not rebuilt; changing the generator's output means bumping `GENERATOR_VERSION` in `tools/repo-gen/src/sets.rs`. The generator only deletes a directory it made itself.

| Set | Disk | Time | Notes |
| --- | --- | --- | --- |
| `target-project` | 1.6 GB | 18 s | Gating. 20 repos and a `workspace.toml` |
| `big-tree` | 2.2 GB | 45 s | Stretch |
| `many-repos` | 3.1 GB | 67 s | Stretch. 200 repos and a `workspace.toml` |
| `big-history` | 7.6 GB | 8 min | Stretch. Clones the Linux kernel at a fixed tag (1.38 million commits); the time depends on the network |

`all` builds all four. Times were measured on a Ryzen 9 3900X (12 cores) with an NVMe SSD.

`just bench` runs the harness in `gitcroc-cli` on `target-project` and prints a JSON array: one object per scenario with the median and 95th percentile in milliseconds, the section 9 budget and whether the median is within it. It exits 1 when a scenario misses its budget. `just bench --stretch` adds the stretch sets that exist locally; `--runs <n>` changes the number of timed runs (10 by default, after one warm-up). Always build in release mode, as `just bench` does.

To add a scenario, add an entry to `SCENARIOS` in `crates/gitcroc-cli/src/bench.rs`. It needs a name, its section 9 budget if it has one, the sets it runs on, and a function that does the untimed setup and returns what to time. `timed(...)` times all the operation does. A `Measured` with a `before` step also runs that step untimed before each run, as `refresh-all` does to start a new core and `group-fetch` to give every remote a new commit.

The multi-repo scenarios of M3:

- `refresh-all`: a core started on a warm summary cache reads every repo again, as after the app's start. Its budget is 2 s on `target-project` and 5 s on `many-repos`.
- `group-history-first-page`: the first page of the merged history of every repo of the set (300 ms on `target-project`).
- `group-fetch` (`many-repos` only, reported): a group fetch of all 200 repos at the default parallelism. It runs on bare remotes and working copies made with `git clone --shared`, so the data set is never written.

Measure on 8 cores, as section 9's reference machine has: on a bigger machine run `taskset -c 0-7 just bench`, and note the RAM.

### Nightly bench workflow

`.github/workflows/bench.yml` runs `just bench` on `target-project` every night and on demand (Actions → Bench → Run workflow), on Linux only. It builds `target-project` once per generator version and caches it. A scenario with a section 9 budget fails the run when its median is over the budget, or more than 20% and also at least 2 ms slower than in `bench/baseline-linux.json`. Hosted runners vary by more than 20% on scenarios of a few milliseconds, so smaller slowdowns, and those of scenarios without a budget, are only reported as `slow`; `gitcroc-cli bench-compare <results.json> <baseline.json>` is the same check, and you can run it locally. A scenario the baseline does not have yet is reported as `new` and passes. The stretch sets never run in CI. The workflow uploads the results as the `bench-linux` artifact and writes the comparison to the run's summary.

Hosted runners have no fixed speed, so this is the only workflow that times anything, and it is not a pull request check. A pull request that changes the workflow runs it once to prove it works, and that run only reports a regression.

### Updating the baseline

The baseline is the CI runner's own speed, not the reference machine's. To make a new one, run the Bench workflow with **baseline** ticked: it runs the bench three times and uploads `baseline-linux.json`, the median of the three (`gitcroc-cli bench-baseline <results.json>...` computes it). The workflow does this on its own when no baseline is committed. Commit the file in a pull request that says why the baseline changed: a deliberate trade-off, a new scenario, a change of runner image. Never update it to make a regression pass.

### UI-side budgets: `just bench-ui`

Some section 9 budgets need the real app. `just bench-ui` measures those that need one repo on the GPUI app and `target-project`, on Linux only (M6-14b): cold start until the tree shows from the cache, selecting the largest repo, a commit, switching between two repos and between two tabs, each until the frame that shows the result, and scrolling the history and a diff. The rest (the refresh after the start, selecting a group, input latency while jobs run, memory) come with M6-29.

1. It builds `gitcroc-next` with the `perf-trace` Cargo feature into `target/bench-ui`, so a measuring build never ends up in a release. With the feature, `gitcroc-next --bench <script> <trace>` plays a script in its window (`crates/gitcroc-ui/src/perf.rs`): real clicks and wheel events at the elements the last frame drew, one step per frame. An action is timed from its click to the present of the first frame that draws its result. A scroll moves 100 px a frame and records the time between frames and how long any history row stayed a placeholder. The feature turns on gpui-kit's `test-support`, whose element registry the runner finds its targets in.
2. `gitcroc-cli bench-ui` sets up a throwaway config and cache: a workspace of the 20 repos with their summaries cached, a second tab of the same repos, and a third tab with one repo whose commit adds a 15,000-line Rust file, since no `target-project` file is long enough to scroll. It starts the app once to warm up and five times to measure (`--runs <n>`), each a new process, and prints JSON like `just bench` does, with a `unit` on each row. It exits 1 when a median is over its budget.

The scroll budget is judged per run: `scroll-history-frame` and `scroll-diff-frame` are the mean time between frames, within 1% of the display's refresh interval (capped at 144 Hz); `-janky` is the share of frames over 1.5 intervals, at most 1%; `scroll-blank` is the longest a row stayed blank. The display's rate comes from the platform, or from `--refresh-hz <hz>`, which X11 needs: an X screen spans every monitor, so GPUI gets no rate there (`taskset -c 0-7 just bench-ui --refresh-hz 144`). GPUI slows the frames of a window that is not active, so the run fails if the window loses the focus while scrolling: leave it in front.

Run it pinned to 8 cores, with nothing else busy, before a release and for a milestone sign-off: `taskset -c 0-7 just bench-ui`. It does not run in CI. `--verbose` also prints each run's samples and scroll runs.

`switch-tab` ends at the other tab's first paint, its tree. `switch-tab-history` is reported without a budget: from the same click until that tab's history, file list and diff are back, which UI-31 reads again. Each run starts from a fresh copy of the config and cache, so the tabs and selection the app saved in one run do not carry over to the next.

## Integration tests and unwrap (QA-4)

`unsafe` is forbidden. `unwrap` and `expect` are denied outside tests. `clippy.toml` allows them in tests, but clippy's `allow-unwrap-in-tests` does not cover helper functions in integration test files. So every integration test file (`tests/*.rs`) starts with:

```rust
#![allow(clippy::unwrap_used, clippy::expect_used)]
```

## Where things are

- Spec (source of truth): `docs/spec/`
- Decisions: `docs/spec/01-decisions.md` and `docs/adr/`
- Tasks: `docs/tasks/`. One task is one pull request.
