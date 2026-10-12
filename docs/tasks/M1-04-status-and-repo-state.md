# M1-04 Working copy status and repo state

## Goal
The core reports a repo's working copy (staged, unstaged, untracked), its diffs, its conflict state and its summary, and the status of the 60,000-file repo meets its budget.

## Depends on
M1-03

## Spec
Implements: ARCH-2, OPS-1
Respects: D3, ARCH-1, ARCH-3, ARCH-5, ARCH-7, ARCH-8, PERF-3, PERF-7, UI-3, UI-10, UI-15, SEC-6, section 9 budgets

## In scope
- `gitcroc-git` read API:
  - `status(repo) -> WorkingCopy`: staged entries (index against `HEAD`), unstaged entries (working tree against index) and untracked files, each with the same id and display-string scheme as M1-03. Unmerged paths are listed separately. Follows git's defaults and the repo's config for untracked files and ignore rules.
  - `file_diff` gains two sources: staged (index against `HEAD`) and unstaged (working tree against index; an untracked file is diffed against nothing).
  - `repo_state(repo) -> RepoState`: conflict state detected from the repo itself, as OPS-1 lists: `MERGE_HEAD`, `rebase-merge`, `rebase-apply`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`, and unmerged index entries. It names the operation in progress, or none.
  - `summary(repo) -> RepoSummary`: current branch or detached `HEAD` with its hash, upstream if set, ahead and behind against the upstream, dirty flag, conflict state. This is what PERF-3 caches and UI-3 shows.
- ARCH-2 fallback: status runs through `gix`. Where `gix` lacks something the repo needs, or gives a different answer than git, it falls back to `git status --porcelain=v2 -z` (and `git diff` for the diff) behind the same trait, so callers never know which ran. The cases that fall back are listed in code with the reason. At least: a repo with `core.fsmonitor` set, and any case the tests below find where `gix` and git disagree. Fallback runs are read-only `git` commands with argument arrays (ARCH-3).
- Git's safety errors, such as `safe.directory`, are returned to the caller with git's message, never bypassed (SEC-6).
- `gitcroc-core`: `working_copy`, `repo_state` and `repo_summary`, off the async worker threads, typed errors.
- `gitcroc-cli status <repo-path>` and `gitcroc-cli summary <repo-path>`.
- Bench scenarios: `status-60k` on the 60,000-file `target-project` repo with git defaults (budget 500 ms), and `status-big-tree` on `big-tree` as a stretch (1 s with fsmonitor and untracked cache on, where the platform supports them; on Linux git has no built-in fsmonitor, so the result is reported with the untracked cache only).
- Tests:
  - `arch_2_status_matches_git`: on a fixture repo with staged, unstaged, untracked, ignored, renamed, deleted and mode-changed files, the `gix` result equals `git status --porcelain=v2` parsed. The same comparison runs on a repo that forces the fallback.
  - `arch_2_callers_cannot_tell_which_ran`: both paths return the same type and the same values.
  - `ops_1_detects_conflict_state` table test: merge, rebase (merge backend), rebase (apply backend), cherry-pick, revert, stash apply with a conflict, and a clean repo. Each conflict is made with `git` in a fixture, as a terminal user would.
  - Summary: on a branch with upstream ahead 2 and behind 1, on a branch without upstream, on a detached `HEAD`, on an unborn branch (new repo with no commits).
  - Unstaged and staged diffs for a modified file, a new untracked file, and a deleted file.

## Out of scope
- Staging, unstaging, discarding and commit (M2). This task only reads.
- Offering to enable `core.fsmonitor` and `core.untrackedCache` (PERF-7): it changes git config, which is a write, so it is M2.
- Caching summaries (M1-06) and refreshing them (M1-11).

## Acceptance
- `gitcroc-cli status` and `gitcroc-cli summary` print the working copy and summary of a repo.
- `just bench` reports `status-60k` under 500 ms on `target-project`; the `big-tree` result is reported in the pull request.
- All tests listed above exist and pass on all three platforms.
