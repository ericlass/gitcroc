# M3-15 Multi-repo budgets, group smoke test and M3 sign-off

## Goal
Milestone M3 is verified: the mixed-branch scenario passes, every section 9 budget is met on `target-project`, and the `many-repos` stretch results are reported.

## Depends on
M3-01 to M3-14

## Spec
Respects: QA-1, QA-3, QA-5, PERF-8, UI-33, section 9 budgets and stretch targets, section 12 "Group scenarios" and "End to end smoke" rows, section 13 M3 row

## In scope
- Bench scenarios for the three M3 rows of section 9:
  - Full status refresh of all 20 repos after start, 2 s: `refresh-all` in `just bench`, from a core with a warm cache to every summary read (the core part), and `refresh-all-ui` in `just bench-ui`, from start to the last repo row losing its stale mark.
  - Switch between two tabs, 100 ms for first paint (UI-33): `switch-tab` in `just bench-ui`. Two workspaces of `target-project` repos, each with its largest repo selected. Each switch is timed from the click to the frame after the history's top row changes.
  - Select a group of all 20 repos, 300 ms: `select-group` from M3-07.
  - Input latency is measured again during a group job: a group fetch of all 20 repos from local bare remotes, then a group pull (`input-latency-group-job`, 50 ms).
  - The nightly gate (PERF-8) covers the new core scenarios with a budget. The baseline is extended by a pull request that says why, as M1-12 did.
- `many-repos` stretch (reported, not gating): cold start 1.5 s, full status refresh 5 s, `group-history-first-page` of all 200 repos, and a group fetch of 200 repos from local bare remotes with the default parallelism of 8, in `just bench --stretch`.
- End-to-end smoke (`just smoke`, Linux, locally and in CI) gains a group flow after the M2 steps:
  1. create a second workspace tab;
  2. add two fixture repos;
  3. create a group of both from a multi-selection;
  4. group checkout of a branch present in one repo only, with the policy "create X from HEAD";
  5. the preflight dialog shows *needs attention* for one repo; Run;
  6. the report shows two outcomes, and the group row shows the branch.
  Each step waits for its result in the DOM, never for a time.
- `docs/tasks/M3-signoff.md`, filled in by the person verifying:
  - every section 9 row on `target-project` with value, budget and pass or fail, the M1 and M2 rows included;
  - the `many-repos` stretch results;
  - the mixed-branch scenario test and the smoke run, with date and commit;
  - Windows and macOS marked deferred, as decided for M0.
- `just coverage M3` joins `just check`, so every M3 requirement has a test (QA-3).
- `CONTRIBUTING.md`: groups and target sets, how a group operation is added (classify and run), and the new bench scenarios.

## Out of scope
- AI (M4). Windows and macOS runs (deferred).

## Acceptance
- `docs/tasks/M3-signoff.md` is filled in for Linux with every budget met, or with each miss turned into a follow-up task, as M1-13 to M1-15 were.
- The smoke test passes locally and in CI, and `just check` enforces M3 coverage and passes.
