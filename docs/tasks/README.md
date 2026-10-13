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

M1-02 and M1-03 are the spikes of section 14 and run first: they carry the
performance risk. If either misses its budget, M1 stops for a decision on D2
and D3 (section 13) before the UI tasks start. M1-02, M1-03 and M1-05 can run in
parallel after M1-01.

## Choices made in these tasks that the spec does not cover

Each is a default so the tasks have no gaps. Veto any of them before the first
task of its milestone starts; a change after that becomes an ADR (DEV-2).

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
| Timing in CI | Hosted CI runners have no fixed CPU, so tests there assert behaviour, never section 9 times: the history scroll test checks that pages load on demand, all arrive and the DOM stays bounded, and only reports how long rows stayed blank. The 100 ms scroll budget is checked on the reference machine by `just bench-ui` (PERF-8, M1-12) | M1-08 |
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
| Libraries | `gix`, `rusqlite` with bundled SQLite, `notify` with a debouncer, CodeMirror 6 with `@codemirror/merge`, TanStack Virtual; versions recorded in each pull request | M1-02 to M1-11 |
