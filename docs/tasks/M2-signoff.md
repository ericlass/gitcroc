# M2 sign-off

Milestone M2 (docs/spec/13-milestones.md) is done when every operation of section 6 has integration tests for success, failure and conflict, a merge conflict can be resolved and continued by hand, and the M1 budgets of section 9 still hold with jobs running. The person verifying fills in this file and commits it.

## Status

Linux is verified below. Windows and macOS are deferred, as decided for M0 on 2026-10-12 (no Windows PC or Mac to measure on): CI runs `just check` there on every pull request, but the smoke test and the benches run on Linux only.

## Operations and their tests

One row per operation of section 6, plus the conflict editor's two writes and the two writes outside section 6. Failure means the job ends *failed* with git's message, or the request is refused with its reason before git runs. Conflict applies to pull, merge, rebase, cherry-pick, revert, stash apply and stash pop; git cannot stop in conflict state in the others. Every test is a git integration test with real `git` on fixture repos, except the two open rows, which test the commands built and the errors reported without starting a file manager or terminal.

The table is `crates/gitcroc-git/tests/ops_matrix.txt`. `ops_matrix_every_operation_has_success_failure_and_conflict_tests` checks that every test named exists under `crates/`, that every row has a success and a failure test, that exactly the seven operations above have conflict tests, and that every `pub async fn` of `gitcroc-git::ops` is in a row.

| Area | Operation | Success | Failure | Conflict |
| --- | --- | --- | --- | --- |
| Repos | Add existing folder | `grp_1_scan_finds_repos_at_depths_1_to_4`, `grp_1_adding_a_repo_twice_is_a_no_op` | `grp_1_a_folder_without_repos_or_missing_is_reported`, `grp_1_scan_refuses_bare_repos_and_skips_symlinks` | n/a |
| Repos | Clone | `grp_1_a_clone_is_added_to_the_workspace`, `grp_1_a_clone_into_an_empty_folder_is_added` | `grp_7_a_failed_clone_is_failed_and_leaves_no_folder`, `grp_1_a_folder_that_is_not_empty_is_refused_before_git_runs` | n/a |
| Repos | Remove from workspace | `grp_3_removing_a_repo_leaves_the_disk_untouched` | `grp_3_removing_a_repo_leaves_the_disk_untouched` | n/a |
| Repos | Open in file manager | `arch_3_open_commands_take_the_path_as_one_argument` | `plat_1_a_repo_folder_that_is_gone_is_reported_and_nothing_starts` | n/a |
| Repos | Open in terminal | `arch_3_open_commands_take_the_path_as_one_argument` | `plat_1_a_missing_terminal_names_what_was_tried`, `plat_1_a_repo_folder_that_is_gone_is_reported_and_nothing_starts` | n/a |
| Working copy | Stage by file | `ui_15_stage_and_unstage_a_file`, `ui_15_stage_and_unstage_several_files_in_one_job` | `ui_15_a_locked_index_fails_the_job_with_gits_message` | n/a |
| Working copy | Stage by hunk | `ui_15_stage_and_unstage_a_hunk` | `ui_15_stale_hunk_is_refused`, `ui_15_hunks_need_the_diff_shown_with_whitespace` | n/a |
| Working copy | Unstage by file | `ui_15_stage_and_unstage_a_file`, `ui_15_unstage_on_an_unborn_branch` | `ui_15_a_locked_index_fails_the_job_with_gits_message` | n/a |
| Working copy | Unstage by hunk | `ui_15_stage_and_unstage_a_hunk` | `ui_15_stale_hunk_is_refused` | n/a |
| Working copy | Discard by file | `ui_15_discard_a_file_and_a_hunk` | `ui_15_a_locked_index_fails_the_job_with_gits_message`, `grp_9_discard_needs_confirmation` | n/a |
| Working copy | Discard by hunk | `ui_15_discard_a_file_and_a_hunk` | `ui_15_stale_hunk_is_refused`, `grp_9_discard_needs_confirmation` | n/a |
| Working copy | Commit | `ui_15_commit_staged_changes`, `sec_6_sign_off_follows_git_config` | `ui_15_commit_without_an_identity_fails_with_gits_message`, `sec_6_hooks_run_as_on_the_command_line` | n/a |
| Working copy | Amend last commit | `ui_15_amend_last_commit` | `ui_15_amend_needs_a_commit_and_a_commit_needs_no_conflict` | n/a |
| Branches | Checkout | `ui_7_checkout_a_local_branch` | `grp_12_checkout_never_stashes`, `ui_7_checkout_a_local_branch` | n/a |
| Branches | Create | `ui_7_create_a_branch` | `ui_7_create_a_branch`, `sec_4_invalid_names_are_refused_before_git_runs` | n/a |
| Branches | Rename | `ui_7_rename_a_branch` | `ui_7_rename_a_branch` | n/a |
| Branches | Delete | `ui_7_delete_a_branch`, `grp_9_remote_delete_needs_confirmation` | `ui_7_delete_a_branch`, `grp_9_force_delete_needs_confirmation` | n/a |
| Branches | Set upstream | `ui_7_set_and_unset_the_upstream` | `ui_7_set_and_unset_the_upstream` | n/a |
| Branches | Checkout of a remote branch | `ui_7_checkout_a_remote_branch` | `ui_7_checkout_a_remote_branch` | n/a |
| Branches | Checkout of a commit | `ui_7_checkout_a_commit_detached` | `ui_7_checkout_a_commit_detached` | n/a |
| Sync | Fetch | `fetch_brings_new_commits_with_progress_then_nothing_to_do` | `fetch_fails_on_an_unreachable_remote`, `sync_is_refused_with_the_reason` | n/a |
| Sync | Pull | `pull_fast_forwards_then_nothing_to_do`, `pull_merges_or_rebases_diverged_branches` | `pull_ff_only_is_failed_on_diverged_branches`, `grp_12_pull_never_stashes` | `ops_1_a_conflicting_merge_pull_ends_conflict`, `ops_1_a_conflicting_rebase_pull_ends_conflict` |
| Sync | Push | `push_to_the_upstream_then_nothing_to_do`, `push_sets_the_upstream_of_a_new_branch` | `push_rejected_as_not_a_fast_forward_is_failed` | n/a |
| Integrate | Merge | `merge_fast_forwards`, `merge_no_ff_makes_a_merge_commit` | `merge_ff_only_fails_on_diverged_history`, `grp_12_merge_never_stashes` | `ops_1_a_conflicting_merge_ends_conflict_and_abort_restores_the_branch` |
| Integrate | Rebase | `rebase_replays_the_branch_onto_the_ref` | `grp_12_merge_never_stashes`, `merge_and_rebase_are_refused_on_a_detached_head_or_an_unknown_ref` | `rebase_stops_at_step_2_of_3_continues_into_a_further_conflict_and_aborts` |
| Integrate | Cherry-pick | `cherry_pick_applies_one_commit`, `cherry_pick_applies_three_commits_oldest_first` | `grp_12_cherry_pick_and_revert_never_stash` | `cherry_pick_conflicts_in_the_second_then_continues` |
| Integrate | Revert | `revert_undoes_a_commit`, `revert_of_a_merge_commit_asks_for_the_mainline` | `grp_12_cherry_pick_and_revert_never_stash` | `a_conflicting_revert_ends_conflict_and_aborts` |
| Integrate | Abort and continue | `merge_continue_commits_the_resolved_merge`, `rebase_continues_to_the_end`, `cherry_pick_abort_restores_the_branch`, `ops_2_abort_follows_the_repo_state` | `abort_and_continue_are_refused_without_an_operation` | n/a |
| Stash | Stash | `stash_with_a_message_and_untracked_files`, `stash_without_a_message_leaves_untracked_files` | `stash_message_with_a_nul_character_is_refused` | n/a |
| Stash | Apply | `stash_apply_keeps_the_entry_and_pop_drops_it` | `grp_12_apply_over_conflicting_local_changes_fails_and_touches_nothing`, `a_stash_list_changed_since_it_was_read_is_refused` | `ops_1_a_conflicting_apply_ends_conflict_and_mark_resolved_ends_the_state` |
| Stash | Pop | `stash_apply_keeps_the_entry_and_pop_drops_it` | `grp_12_apply_over_conflicting_local_changes_fails_and_touches_nothing`, `a_stash_list_changed_since_it_was_read_is_refused` | `a_conflicting_pop_keeps_the_stash` |
| Stash | Drop | `stash_drop_needs_confirmation` | `a_stash_list_changed_since_it_was_read_is_refused` | n/a |
| Tags | Create | `create_annotated_and_lightweight_tags_at_head_or_a_commit`, `create_with_push_creates_and_pushes_the_tag_as_one_job` | `create_tag_refuses_an_existing_name_a_missing_commit_and_a_blank_message`, `sec_4_invalid_tag_names_are_refused_before_git_runs` | n/a |
| Tags | Delete | `delete_tag_locally`, `delete_tag_also_on_the_remote_after_a_confirmation` | `delete_on_an_unreachable_remote_fails_and_keeps_the_local_tag` | n/a |
| Tags | Push | `push_tag_to_a_file_remote_and_fail_on_a_different_tag_of_that_name` | `push_tag_to_a_file_remote_and_fail_on_a_different_tag_of_that_name`, `push_tag_asks_for_a_remote_when_there_is_no_origin_and_several` | n/a |
| Reset | Soft | `reset_soft_moves_the_branch_and_keeps_the_index_and_the_working_tree` | `reset_refuses_what_names_no_commit_and_a_repo_in_conflict_state` | n/a |
| Reset | Mixed | `reset_mixed_resets_the_index_and_keeps_the_working_tree` | `reset_refuses_what_names_no_commit_and_a_repo_in_conflict_state` | n/a |
| Reset | Hard | `grp_9_hard_reset_needs_confirmation`, `reset_of_a_detached_head_resets_head_and_says_so` | `reset_refuses_what_names_no_commit_and_a_repo_in_conflict_state` | n/a |
| Remotes | Add | `add_a_remote_and_fetch_it_as_one_job_or_only_add_it` | `add_with_a_fetch_that_fails_fails_and_keeps_the_remote`, `sec_4_add_refuses_a_taken_or_invalid_name_and_an_unusable_url_before_git_runs` | n/a |
| Remotes | Rename | `rename_keeps_upstreams_pointing_at_the_renamed_remote` | `rename_refuses_a_missing_remote_a_taken_name_and_an_invalid_one` | n/a |
| Remotes | Remove | `remove_asks_first_naming_the_upstreams_it_unsets` | `remove_without_upstreams_still_asks_and_a_missing_remote_is_refused` | n/a |
| Remotes | Edit URL | `set_url_changes_the_fetch_url_and_a_separate_push_url` | `set_url_refuses_a_missing_remote_and_an_unusable_url` | n/a |
| Conflict state | Mark resolved | `ops_5_mark_resolved_writes_and_stages` | `ops_5_mark_resolved_refuses_markers`, `ops_5_a_conflict_resolved_meanwhile_is_refused_as_already_resolved` | n/a |
| Conflict state | Choose a version | `ops_6_choice_conflicts` | `ops_5_a_conflict_resolved_meanwhile_is_refused_as_already_resolved` | n/a |
| Not in section 6 | Commit-graph cache (PERF-2) | `arch_1_commit_graph_is_written_by_the_git_cli` | `arch_1_a_failed_commit_graph_write_is_reported` | n/a |
| Not in section 6 | File system monitor settings (PERF-7) | `perf_7_never_changes_git_config_without_asking` | `grp_8_turning_the_settings_on_is_refused_in_conflict_state` | n/a |

Added in M2-15 to fill gaps: `grp_12_cherry_pick_and_revert_never_stash` (cherry-pick and revert had no failure test); `ui_15_a_locked_index_fails_the_job_with_gits_message` now covers unstage and discard too, `ui_15_stale_hunk_is_refused` covers discard and unstage of a hunk, and `grp_12_apply_over_conflicting_local_changes_fails_and_touches_nothing` covers apply as well as pop. `plat_1_a_repo_folder_that_is_gone_is_reported_and_nothing_starts` came with a fix: opening a repo whose folder was deleted reported "none of xdg-open was found" instead of the missing folder.

Requirement coverage: `just coverage M2` reports no missing requirement and is part of `just check`.

## End-to-end run (Linux)

`just smoke` (`ui/webdriver/smoke.ts`), run under `xvfb-run -a`, on the same machine as the benches:

1. The app starts on a fixture workspace with one repo, `lib`.
2. **Add repository → Add folder…** adds `app` (an `e2e` build answers the folder dialog with `GITCROC_E2E_PICK_FOLDER`, since WebDriver cannot drive a native dialog).
3. **Stage notes.txt**, then a commit "Add notes" from the commit box.
4. **Merge** `feature` into `main` from the toolbar: the job ends *conflict* and the banner shows.
5. In the conflict editor: **Take theirs in conflict 1**, then a typed edit at the end of the line; **Mark resolved**; **Continue**.
6. The history shows "Merge branch 'feature'", the uncommitted changes row is gone, and `git status` is clean; `HEAD` has two parents and `a.txt` holds the edited text.

Every step waits for its result in the DOM; none waits for a time. Result: passed, 4 of 4 runs on the measured commit, about 4 s each.

- Date and commit: 2026-10-15, branch `m2-15-signoff` (the commit of this file)
- CI: the Linux job runs `xvfb-run -a just smoke` after `just check`.

## Machine (Linux)

The M1 sign-off's machine:

- CPU: AMD Ryzen 9 3900X, pinned to 8 threads with `taskset -c 0-7`
- RAM: 32 GB
- Disk: Samsung SSD 970 EVO Plus 1 TB, NVMe
- OS: Ubuntu 24.04.5 LTS, kernel 6.8.0-147, WebKitGTK 2.52.6
- git: 2.43.0
- Commit measured: branch `m2-15-signoff`, 2026-10-15
- Measured by, date: Claude for Eric Lass, 2026-10-15

## Budgets against the M1 sign-off (Linux, `target-project`)

`taskset -c 0-7 just bench-ui --runs 3` and `taskset -c 0-7 just bench --runs 3`. M1's column is its sign-off table (`--runs 10`). Three runs give a rougher median than ten; every row is within its budget and within the spread M1 reported between runs.

| Section 9 row | Scenario | M1 median | M2 median | M2 p95 | Budget | Pass |
| --- | --- | --- | --- | --- | --- | --- |
| Cold start: tree visible from cache | `bench-ui` `cold-start` | 596 ms | 606 ms | 619 ms | 1 s | pass |
| Select the largest repo: first page of history visible | `bench-ui` `select-largest-repo` | 93.5 ms | 102 ms | 105 ms | 200 ms | pass |
| Scroll history at any speed: 60 fps | `bench-ui` `scroll-frame` | 16 ms | 16 ms | 17 ms | 16.7 ms | pass |
| Scroll history: no row blank for more than 100 ms | `bench-ui` `scroll-blank` | 0 ms | 0 ms | 0 ms | 100 ms | pass |
| Select a commit with under 5,000 changed lines | `bench-ui` `select-commit` | 95.5 ms | 96 ms | 164 ms | 150 ms | pass |
| Switch selection between two repos: first paint | `bench-ui` `switch-repo` | 72.5 ms | 75.5 ms | 109 ms | 100 ms | pass |
| Working copy status of the 60,000-file repo | `bench` `status-60k` | 68 ms | 69 ms | 76 ms | 500 ms | pass |
| UI input latency while any job runs: a refresh | `bench-ui` `input-latency` | 26 ms | 26 ms | 38 ms | 50 ms | pass |
| UI input latency while any job runs: fetch and pull | `bench-ui` `input-latency-job` | (new) | 19 ms | 25 ms | 50 ms | pass |
| Memory, workspace idle, including the webview | `bench-ui` `memory-idle` | 370 MB | 378 MB | 389 MB | 400 MB | pass |

Pass is decided by the median, as in M1. The p95 of `select-commit` and `switch-repo` is over the budget in three runs, as it was in M1 (158 and 102 ms).

`input-latency-job` is new in M2-15. It types the same filter key as `input-latency` while real jobs run, and counts only keys that land while the status bar shows a job running (60 keys). The jobs come in waves: a fetch of all 20 repos from local bare remotes, then a pull of the largest repo (`repo-01`, 60,000 files), again and again; before each wave every remote gets a new commit, so each fetch brings objects and each pull fast-forwards and updates the working tree. The repos are working copies with bare remotes made for the run with `git clone --shared`, so the reference data is never written. `just bench-ui --job-only` measures this scenario alone.

Core-side parts from `just bench`, for comparison with M1: `cold-start-core` 3.3 ms (M1 2.0), `history-first-page` 21 ms (21), `select-commit` 13 ms (13).

### Missed budgets

None.
