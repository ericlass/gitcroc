# M6-19c4 Dropping conflict state, closing tabs, restart and keyboard

Fourth implementation PR of [M6-19c](M6-19c-conflict-flow-and-jobs.md),
under DEV-4. Depends on [M6-19c3](M6-19c3-edited-outside-and-choices.md).

Implements: OPS-9, UI-29 (lifecycle of kept conflict state)
Respects: OPS-7, UI-22, UI-30, UI-31, ARCH-9, GRP-9, SEC-3, D15, QA-2, QA-3, QA-4, QA-5

Kept editors survive file, repo and tab switches and background jobs; the
background release of diffs leaves them. A repo's kept state is dropped
when its Abort job succeeds or a fresh loaded summary shows its conflict
ended; refreshing, stale or missing summaries and failed Aborts keep it.
Closing a tab with unsaved conflict edits asks first, naming the files;
Cancel keeps everything, confirming closes the tab and releases its
editors. Installing an update asks first when any tab has unsaved conflict
edits, listing workspace, repo and file; Cancel leaves everything, and only
explicit confirmation runs the existing install and restart. A conflicting
stash pop resolves through these flows and keeps M6-18's Keep/Drop offer.
Enter or Space on an unmerged row moves the focus into its conflict view,
and Ctrl or Cmd and Enter marks it resolved, so files can be resolved one
after another from the keyboard.

Write first: `ui_29_conflict_edits_survive_file_repo_and_tab_switches`,
`ops_9_abort_and_external_resolution_clear_only_finished_state`,
`ui_29_closing_unsaved_conflict_tab_asks_before_discarding`,
`ops_9_update_restart_asks_about_unsaved_edits_in_all_tabs`,
`ops_7_conflicting_pop_offer_follows_resolution` and
`ui_22_resolve_two_files_and_continue_using_only_the_keyboard`.

Acceptance: these tests and `just check` pass, completing M6-19c.
