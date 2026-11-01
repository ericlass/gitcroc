# M6-19c2 Unmerged files in the kept editor, and Mark resolved

Second implementation PR of [M6-19c](M6-19c-conflict-flow-and-jobs.md),
under DEV-4. Depends on [M6-19c1](M6-19c1-conflict-banner.md).

Implements: OPS-3, OPS-5, OPS-9 (opening, editing and resolving text conflicts)
Respects: OPS-7, OPS-8, ARCH-6, ARCH-7, ARCH-9, UI-22, SEC-3, D15, QA-2, QA-3, QA-4, QA-5

Selecting an unmerged file reads it with `conflict_get` and shows M6-19a/b's
kept editor in place of the diff, kept per workspace, repo and path in the
tab's changes pane. Unmerged rows show an unsaved mark. Mark resolved sends
the whole buffer with `conflict_resolve`; a markers-remain refusal names the
line and offers Mark resolved anyway, which ordinary Enter does not press.
Show request and job errors inline and keep the buffer; no second request
goes while one is pending or its job runs. Requests always use the current
view's handle and file id for the kept path; a replaced view or an
already-resolved file reads the lists again. A fresh list without a kept
path drops it. When the resolved file was selected, the next unmerged file
is selected.

Write first: `ops_3_unmerged_files_open_first_and_show_unsaved_marks`,
`ops_5_mark_resolved_stages_only_on_request`,
`ops_5_marker_refusal_requires_explicit_override`,
`arch_9_stale_conflict_reads_and_resolve_handles_cannot_cross_targets` and
`ops_2_continue_waits_for_all_unmerged_paths`.

Acceptance: these tests and `just check` pass.
