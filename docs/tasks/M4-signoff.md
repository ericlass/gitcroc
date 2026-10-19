# M4 sign-off

Milestone M4 (docs/spec/13-milestones.md) is done when every AI requirement has a test against the mock provider, and the fixture set (AI-15) has run against one API provider and one CLI provider. The person verifying fills in this file and commits it.

## Status

Linux is verified below. Windows and macOS are deferred, as decided for M0 on 2026-10-12 (no Windows PC or Mac to measure on): CI runs `just check` there on every pull request, but the smoke test and the benches run on Linux only.

## Requirement coverage (QA-3)

- [x] `just check` runs `just coverage M4`, and M4 has no requirement without a test: 21 covered, 0 missing.
- [x] Every AI test runs against the mock provider, the fake HTTP server, `fake-cli` or the CLI's `--mock` replies (section 12 "AI"); none calls a real provider, and none waits for a time.

## Fixture runs (AI-15)

`just ai-eval` on the 36 fixtures of `crates/gitcroc-ai/fixtures/conflicts/`, by hand on the developer's machine with Eric's accounts. Keys come from an environment variable Eric set; no agent typed one, and none is in the repo (SEC-1). A hunk *matches* when its proposal equals its part of an accepted resolution; a file is *right* when every hunk matches or is correctly left unresolved.

| | API provider | CLI provider |
| --- | --- | --- |
| Date | pending | 2026-10-19 |
| Commit | pending | 2f31e99 (branch `m4-11-prompt-fixtures`, merged as 0d5a6cc) |
| Command | pending | `just ai-eval --kind claude_code --path "$(which claude)"` |
| Provider | pending | Claude Code CLI |
| Model | pending | Claude Code's default model (none given) |
| Prompt version | v1 | v1 |
| Fixtures right | pending | 36 of 36 |
| Hunks | pending | 38 |
| Matches | pending | 30 |
| Correct unresolved | pending | 8 |
| Unexpected unresolved | pending | 0 |
| Differs | pending | 0 |
| AI failed | pending | 0 |
| Requests | pending | 36 |
| Tokens in / out | pending | 159,053 / 6,242 |

Confidence against score, CLI run: high, 29 matches and 7 correct unresolved; medium, 1 match (32) and 1 correct unresolved (33); low, none.

A perfect score leaves no room on this provider to show a better prompt. Whether to add harder fixtures or a prompt version is Eric's call, as a follow-up task.

## End-to-end run (Linux)

`just smoke` (`ui/webdriver/smoke.ts`), run under `xvfb-run -a`. After M2's steps and M3's group flow, the AI flow, in the second tab:

1. `app`, on `feature` since the group checkout, merges `topic`, which conflicts in `a.txt`; the conflict editor shows **Resolve with AI**.
2. Settings, AI: **Add provider…** of the kind **Mock** (offered by the `e2e` build with `GITCROC_E2E_AI`, which names the scripted reply), then **Set mock as default** (the provider is named `mock`).
3. **AI features** off: the conflict editor has no **Resolve with AI**. On again (AI-16).
4. **Resolve with AI**; the consent dialog *Send to mock?* appears; **Allow** (AI-6).
5. The proposal shows; **Accept all high-confidence**. The editor reads `feature and topic`, and git still has `a.txt` in conflict (AI-12).
6. **Mark resolved**, then **Continue**. The history shows the merge; git has no `MERGE_HEAD`, `HEAD` has two parents, and `a.txt` holds the proposed text.

Every step waits for its result in the DOM; none waits for a time. Result: passed, 4 of 4 runs.

- Date and commit: 2026-10-19, branch `m4-12-signoff` (the commit of this file)
- CI: the Linux job runs `xvfb-run -a just smoke` after `just check`.

## Budgets (Linux, `target-project`)

The M2 and M3 sign-offs' machine: AMD Ryzen 9 3900X pinned to 8 threads with `taskset -c 0-7`, 32 GB, Samsung SSD 970 EVO Plus, Ubuntu 24.04.5 LTS, kernel 6.8.0-147, WebKitGTK 2.52.6, git 2.43.0. Measured by Claude for Eric Lass, 2026-10-19, on `main` at 0d5a6cc, in a worktree of its own: this branch adds only the `e2e` build's mock and the kind list to it.

`taskset -c 0-7 just bench-ui --runs 3` and `taskset -c 0-7 just bench --runs 3`. The M3 column is the M3 sign-off's table. Pass is decided by the median, as before.

| Section 9 row | Scenario | M3 median | M4 median | M4 p95 | Budget | Pass |
| --- | --- | --- | --- | --- | --- | --- |
| Cold start: tree visible from cache | `bench-ui` `cold-start` | 617 ms | 627 ms | 643 ms | 1 s | pass |
| Full status refresh of all 20 repos after start | `bench-ui` `refresh-all-ui` | 1,048 ms | 1,096 ms | 1,129 ms | 2 s | pass |
| Full status refresh, the core part | `bench` `refresh-all` | 781 ms | 854 ms | 866 ms | 2 s | pass |
| Select the largest repo: first page of history visible | `bench-ui` `select-largest-repo` | 101 ms | 97 ms | 99 ms | 200 ms | pass |
| Scroll history at any speed: 60 fps | `bench-ui` `scroll-frame` | 16 ms | 16 ms | 17 ms | 16.7 ms | pass |
| Scroll history: no row blank for more than 100 ms | `bench-ui` `scroll-blank` | 0 ms | 0 ms | 0 ms | 100 ms | pass |
| Select a commit with under 5,000 changed lines | `bench-ui` `select-commit` | 103 ms | 98 ms | 164 ms | 150 ms | pass |
| Switch selection between two repos: first paint | `bench-ui` `switch-repo` | 88.5 ms | 86 ms | 122 ms | 100 ms | pass |
| Switch between two tabs: first paint (UI-33) | `bench-ui` `switch-tab` | 86.5 ms | 84.5 ms | 98 ms | 100 ms | pass |
| Select a group of all 20 repos: first page of merged history | `bench-ui` `select-group` | 143 ms | 139 ms | 141 ms | 300 ms | pass |
| Select a group of all 20 repos, the core part | `bench` `group-history-first-page` | 68 ms | 75.5 ms | 77 ms | 300 ms | pass |
| Working copy status of the 60,000-file repo | `bench` `status-60k` | 74 ms | 81 ms | 83 ms | 500 ms | pass |
| UI input latency while any job runs: a refresh | `bench-ui` `input-latency` | 30.5 ms | 29.5 ms | 37 ms | 50 ms | pass |
| UI input latency while any job runs: fetch and pull | `bench-ui` `input-latency-job` | 16 ms | 16 ms | 26 ms | 50 ms | pass |
| UI input latency while any job runs: a group fetch and a group pull | `bench-ui` `input-latency-group-job` | 25 ms | 24 ms | 58 ms | 50 ms | pass |
| Memory, workspace idle, including the webview | `bench-ui` `memory-idle` | 358 MB | 441 MB | 446 MB | 400 MB | **fail** |
| Memory, with a second tab in the background (UI-31) | `bench-ui` `memory-two-tabs` | 376 MB | 456 MB | 466 MB | 400 MB | **fail** |

The p95 of `select-commit`, `switch-repo` and `input-latency-group-job` is over the budget in three runs; the first two were in M1 to M3 too.

### Missed budgets

- **Memory, idle and with a second tab: 441 MB and 456 MB, of 400 MB.** M4 caused it. On 2026-10-19, `just bench-ui --runs 1 --verbose` on the same machine and data measured the M3 commit (b5824b9) at 358 MB and 379 MB, and `main` at 442 MB and 463 MB. Both processes grew. The gitcroc process went from 151–189 MB to 185–199 MB, and the WebKit web process from 189–203 MB to 232–260 MB. The follow-up is [M4-13](M4-13-idle-memory.md): find the commit and what it holds, and bring both rows within budget.

## Keychain (Linux, by hand)

- [ ] Eric, in the app with the OS keychain (`just dev`, not the `e2e` build): add an Anthropic provider with a key, then check that the key is in the Secret Service (for example `secret-tool search --all service gitcroc`) and nowhere in `settings.toml` or the log; remove the provider and check that the key is gone. Date: pending.

## Windows (deferred)

| Check | Result |
| --- | --- |
| Fixture runs, smoke test, budgets, keychain | deferred |

## macOS on Apple silicon (deferred)

| Check | Result |
| --- | --- |
| Fixture runs, smoke test, budgets, keychain | deferred |
