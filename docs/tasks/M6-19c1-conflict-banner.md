# M6-19c1 Conflict banner, Abort and Continue

First implementation PR of [M6-19c](M6-19c-conflict-flow-and-jobs.md), under
DEV-4. Read the parent's sources, scope and cited requirements as well.

Implements: OPS-2, UI-10 (the banner and its actions)
Respects: OPS-1, OPS-7, GRP-8, GRP-9, ARCH-6, ARCH-9, UI-22, SEC-3, D15, QA-2, QA-3, QA-4, QA-5

Above a single repo's history, show the operation, applying/onto labels,
rebase/mailbox step and unmerged count from fresh core state, for conflicts
started in a terminal or in the app. Read `conflict_details` again after
each fresh summary; a failed read leaves the banner and its actions usable
with an inline error, and a reply for another repo or operation cannot
replace what is shown. Abort and Continue go through M6-18's operation
dialog with `op_integrate`, so the core's confirmation of a stash-apply
Abort and its refusals appear there, Cancel first. Continue is disabled
with its reason while paths are unmerged or a job runs on the repo; a
stash apply has no Continue to offer.

Write first: `ops_2_banner_tracks_terminal_and_app_conflicts` and
`grp_9_stash_abort_requires_confirmation`.

Acceptance: both tests and `just check` pass.
