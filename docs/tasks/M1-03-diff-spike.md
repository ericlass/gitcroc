# M1-03 Diff spike: changed files and one file's diff

## Goal
The core returns a commit's header and changed files, and the diff of one file, fast enough to meet the "select a commit" budget, and `gitcroc-cli diff` prints them.

This is a spike like M1-02: if the budget cannot be met with `gix`, stop and report the measurements before M1-09 starts.

## Depends on
M1-01

## Spec
Implements: ARCH-8 (diff half; M1-02 has the history half)
Respects: D3, ARCH-2, ARCH-5, ARCH-7, UI-14, UI-16, UI-17, UI-18, PLAT-2, SEC-3, section 9 budgets

## In scope
- `gitcroc-git` read API, behind the same kind of trait as M1-02 (ARCH-2):
  - `commit(repo, hash) -> CommitDetails`: full hash, author and committer with dates, full message, parent hashes.
  - `changed_files(repo, hash, parent_index) -> Vec<ChangedFile>`: the tree diff against the chosen parent (first parent by default, UI-16), or against the empty tree for a root commit. Each entry: path, old path for renames, status (added, modified, deleted, renamed, type changed), and added and removed line counts when cheap, otherwise absent.
  - Rename detection follows the repo's `diff.renames` config, with git's default when unset.
  - `file_diff(repo, source, file, options) -> FileDiff` for exactly one file (ARCH-8). `source` is a commit and parent here; M1-04 adds the working copy sources. Options: ignore whitespace, context lines (3, 10, whole file).
  - `FileDiff` is one of:
    - text: hunks with their line ranges and lines (context, added, removed), plus both sides' full text when the context is "whole file" (side-by-side mode needs it);
    - binary: old and new size;
    - image: old and new bytes for PNG, JPEG, GIF, WebP and BMP up to 10 MB per side, otherwise binary;
    - too large: the counts only, when the diff has over 20,000 changed lines, unless the caller passes "load anyway" (UI-18);
    - submodule: old and new commit hash.
  - Binary detection follows git: the `binary` and `-diff` attributes first, then a NUL byte in the first 8,000 bytes. SVG is text.
  - Text that is not UTF-8 is decoded lossily for display (SEC-3).
- How paths cross IPC: the core keeps paths as `PathBuf` or git's bytes (PLAT-2). Every changed file carries a display string and an id that is its index in the list; callers ask for a file's diff by id, never by sending a path string back.
- `gitcroc-core`: `commit_details`, `changed_files` and `file_diff`, off the async worker threads, typed errors.
- `gitcroc-cli diff <repo-path> <hash> [--parent <n>] [--file <id>] [--ignore-whitespace] [--context <3|10|all>]`.
- Bench scenario `select-commit` on `target-project`: header plus changed files plus the first file's diff for a commit with under 5,000 changed lines (budget 150 ms for the core part). Also `select-commit-large` for the largest commit in the set, reported only.
- Tests:
  - `arch_8_diff_is_one_file_at_a_time`: the API has no call that returns more than one file's diff, and `changed_files` returns no content.
  - Table tests over fixture commits: add, modify, delete, rename (with `diff.renames` on and off), mode change, root commit, merge commit against parent 1 and parent 2.
  - Binary by NUL byte, binary by attribute, image, too large and "load anyway", submodule.
  - Ignore whitespace and each context option.
  - A file name with spaces, quotes and non-ASCII characters round-trips through the id mapping (PLAT-2).

## Out of scope
- Working copy status and uncommitted diffs (M1-04).
- The changes pane and CodeMirror (M1-09).
- Syntax highlighting (done in the UI by CodeMirror, M1-09).

## Acceptance
- `gitcroc-cli diff` prints a commit's files and one file's diff.
- `just bench` reports `select-commit` under 150 ms on `target-project`.
- If the budget is missed, the pull request says so with the numbers, and M1 stops for a decision.
- All tests listed above exist and pass on all three platforms.
