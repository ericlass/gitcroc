# M1-01 Reference data and bench harness

## Goal
`just bench-data` generates the reference data of section 9 on any machine, and `just bench` runs a harness in `gitcroc-cli` that later tasks add their scenarios to.

## Depends on
M0 (complete)

## Spec
Implements: QA-2
Respects: PERF-8, section 9 "Reference data" table, D5, ARCH-1, ARCH-3, ARCH-5

## In scope
- `tools/repo-gen`, a new tools crate like `tools/req-coverage` (not a product crate, so it is outside the section 4 Crates table):
  - A library of fixture helpers for tests (QA-2): create a repo in a temp dir, write and commit files, create branches, tags, remotes (as a second local repo), stashes, merge commits and conflicts. Every repo write goes through `git` with argument arrays (ARCH-1, ARCH-3), never `gix` writes. The product crates use it as a dev-dependency only.
  - A binary that builds the generated reference sets by writing a `git fast-import` stream to the stdin of `git fast-import`. Output is identical for identical input: a fixed seed, fixed author, committer and dates.
    - `target-project`: 20 repos in 4 groups, 200,000 tracked files and 2 million lines in total, one repo holding 60,000 of the files, 5,000 commits and 30 branches per repo. It also writes a workspace file (section 10 format) listing the 20 repos and 4 groups, for M3.
    - `big-tree`: one repo with 500,000 tracked files.
    - `many-repos`: 200 repos with 2,000 commits and 20 branches each, in 10 groups, plus its workspace file.
    - Commits touch a realistic share of files: most change 1 to 20 files, a few change hundreds; some commits are merges, so history has a graph to draw.
    - Each generated repo gets a commit-graph file (`git commit-graph write --reachable`), since real clones usually have one and `gix` uses it.
  - `big-history` is not generated: `just bench-data big-history` clones the Linux kernel repository and checks out a fixed release tag, recorded in the generator. It is never run in CI.
- `just bench-data <set>` (sets: `target-project`, `big-tree`, `many-repos`, `big-history`, `all`). Data goes to `target/bench-data/<set>`, or to `$GITCROC_BENCH_DATA/<set>` when that is set. A set whose generator version has not changed is not rebuilt.
- `gitcroc-cli bench`: the harness (PERF-8).
  - Runs named scenarios against a data set, each a fixed number of times after one warm-up run, and prints JSON: scenario, data set, median, p95, budget, pass or fail.
  - `just bench` calls it for `target-project`. `just bench --stretch` adds the stretch sets that exist locally.
  - This task adds the scenario framework and one scenario, `open-repo` (open the largest `target-project` repo with `gix`), so the plumbing is proven. M1-02 to M1-06 add the real scenarios.
- Remove the "not available until M1" stub from `just bench` and the matching note in `CLAUDE.md`.
- Document `just bench-data` and `just bench` in `CONTRIBUTING.md`, including disk space and generation time measured on the developer's machine.
- Tests:
  - `qa_2_fixture_helpers_build_repos_in_code`: the helpers build a repo with two branches, a tag, a merge and a stash, and `git` reads them back.
  - A test that generating a small set twice (a reduced size passed as a parameter) gives byte-identical commit ids.
  - A test that the harness's JSON output parses and contains the `open-repo` scenario, run against a small generated set.

## Out of scope
- Any read API beyond opening a repo (M1-02 onward).
- Running the harness in CI and the 20% regression gate (M1-12).
- UI-side measurements (M1-12).

## Acceptance
- `just bench-data target-project` builds the set and a second run is a no-op.
- `just bench` prints the `open-repo` result for `target-project`.
- All tests listed above exist and pass on all three platforms.
