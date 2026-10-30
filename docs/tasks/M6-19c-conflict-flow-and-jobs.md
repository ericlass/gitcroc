# M6-19c Conflict banner, file switching and resolution jobs

## Goal
Resolve a real repo conflict in the native UI: open unmerged files, keep edits
while switching, Mark resolved, then Continue or Abort through core jobs.
This is the final M6-19 slice.

## Depends on
- M6-19a/b: retained editor and its visible hunk controls.
- M6-16: stage/commit, working-copy views and job refresh.
- M6-18: operation dialogs, integrate requests and retained-stash offer.
- M6-09/M6-14: tab switching, cleanup and background job events.
- M6-15b: update dialog's deferred unsaved-edits restart question.

## Spec
Implements: OPS-2, OPS-3, OPS-5, OPS-6, OPS-8, OPS-9, UI-10, UI-29 (conflict flow)
Respects: OPS-1, OPS-4, OPS-7, ARCH-1, ARCH-3, ARCH-6, ARCH-7, ARCH-9, GRP-8, GRP-9, UI-22, UI-30, UI-31, SEC-3, D15, QA-2, QA-3, QA-4, QA-5

Read all cited requirements before implementation, including the conflict and
integrate service types. UI state renders core results; it does not infer
conflict kind, parse disk markers, choose Git commands or duplicate policy.

## Sources and integration
- Port `ui/src/integrate/ConflictBanner.tsx`, its non-AI behavior in
  `ui/src/stores/integrate.ts`, `ui/src/conflict/ConflictView.tsx`, the non-AI
  parts of `ui/src/stores/conflicts.ts`, and changes/tab/restart wiring from
  `ui/src/App.tsx` and rc.1's update dialog.
  Read `ui/src/conflict/conflict.test.tsx` and
  `ui/src/integrate/integrate.test.tsx` for behavior.
- Use `App::conflict_details`, `conflict_get`, `conflict_resolve`,
  `conflict_choose` and `op_integrate` with the existing typed requests and
  replies. Reuse M6-18's operation dialog for core-confirmed Abort and its
  inline refusal handling. Any modal uses `dialog.rs` and the window's
  `open_dialog` pattern, including focus restoration and Cancel-first safety.

## In scope
- Above a single repo's history, show the current operation, applying/onto
  labels, rebase/mailbox step when available, and unmerged count from fresh
  core state. Terminal-started conflicts work like app-started ones. Detail
  read failure leaves the banner and actions usable with an inline error;
  stale replies from another workspace/repo/operation cannot replace it.
- Abort/Continue call `op_integrate` for the selected workspace/repo. Disable
  duplicate submissions and unavailable actions with reasons. Continue cannot
  run with unmerged paths; core rechecks all conditions. Preserve rc.1's stash
  apply semantics: no sequencer step remains after its last path is resolved;
  destructive Abort requires the core's explicit confirmation. Cancel makes
  no write and Enter in an unrelated field cannot confirm it.
- Unmerged files come before Staged/Unstaged, with conflict kind and unsaved
  mark. Selecting one replaces the diff with M6-19a/b's kept editor. Preserve
  ordinary diff, stage/unstage and commit behavior outside conflict editing.
- On `edited_outside`, ask Keep file as it is or Start over from conflict.
  Keep disk opens plain editable disk text without hunk actions; disable it
  when the core cannot supply disk text. Start over uses core index chunks.
  Neither answer writes a file. Keep the answer with the file's editor state.
- Choice conflicts show both core-labelled versions, reason and side metadata
  (deleted, size/mode, symlink or submodule commit as supplied), with Use this
  version calling `conflict_choose`. No text editor for these conflicts.
- Mark resolved sends the current whole buffer through `conflict_resolve`.
  A markers-remain refusal names the first line and offers an explicit Mark
  resolved anyway action; ordinary Enter must not silently override. Show
  request/job errors inline and retain edits after failure. Avoid duplicate
  jobs while a resolve/choose operation is pending or running.
- Handle stale changes handles and already-resolved replies by rereading the
  current changes view, without applying old IDs to new files. Fresh views
  must supply current request handles while retained buffers keep their text.
  After jobs, let core refresh remove resolved files and update Continue.
- Keep buffers, cursor, scroll, undo and card state across file/repo/tab
  switches, including background jobs. Loaded-state cleanup for background
  diffs must not discard unsaved conflict entities. Drop a file when a fresh
  unmerged list no longer contains it; drop a repo when a completed Abort or
  fresh loaded summary establishes that its conflict ended. A transient stale,
  null or refreshing summary must not discard edits, and failed Abort retains
  them. Terminal resolution/abort is reconciled on refresh.
- Add the unsaved-conflict question deferred by M6-09 before closing a tab.
  Cancel keeps the tab and edits; explicit discard closes it and releases its
  editor entities. Reuse rc.1's close behavior and the native dialog pattern.
- Add M6-15b's deferred question before restarting to install an update when
  any tab has unsaved conflict edits. List the affected workspace/repo/files;
  Cancel leaves the app and buffers intact. Restart requires explicit consent
  to lose the edits. Use the existing updater service rather than a second
  restart path.
- Resolve a conflicting stash pop through these flows and retain M6-18's
  Keep/Drop offer after the refreshed repo leaves conflict state.
- All actions are keyboard reachable; resolve successive files without losing
  focus. Keep repo paths, labels and core error text literal. Externalize UI
  strings. Group conflict selection continues to use existing per-repo
  selection; no group resolution operation is introduced.

## Tests (QA-3)
Write these first as GPUI flows against real fixture repos and app services:
- `ops_2_banner_tracks_terminal_and_app_conflicts`: merge, rebase, cherry-pick,
  revert, mailbox and stash-apply state; applying/onto/step and selected repo.
- `ops_2_continue_waits_for_all_unmerged_paths`: resolve two files with the
  keyboard, then Continue finishes; exercise sequencer steps and inline errors.
- `ops_3_unmerged_files_open_first_and_show_unsaved_marks`.
- `ops_5_mark_resolved_stages_only_on_request`: disk/index unchanged beforehand,
  correct final text afterward, job failures preserve the buffer.
- `ops_5_marker_refusal_requires_explicit_override`: first marker line shown,
  no write on refusal, override stages, focused controls keep Enter/Space.
- `ops_6_choice_conflicts_show_and_use_core_versions`: binary, deletion, mode
  conflict and submodule; both side actions and literal metadata.
- `ops_8_edited_outside_requires_a_choice`: Keep disk and Start over, including
  unavailable disk text and marker-like disk text that creates no fake hunks.
- `ui_29_conflict_edits_survive_file_repo_and_tab_switches`: text, cursor,
  selection, scroll, undo and base/card state; identical paths in different keys.
- `ops_9_abort_and_external_resolution_clear_only_finished_state`: successful
  and failed Abort, terminal abort/resolve, refreshing/null summaries and
  background job completion; unrelated repo/workspace buffers survive.
- `ui_29_closing_unsaved_conflict_tab_asks_before_discarding`: Cancel keeps
  exact state; confirmed close releases it.
- `ops_9_update_restart_asks_about_unsaved_edits_in_all_tabs`: Cancel does not
  invoke restart; explicit confirmation uses the existing updater path. Use
  the updater test seam so tests do not replace or restart the test process.
- `arch_9_stale_conflict_reads_and_resolve_handles_cannot_cross_targets`.
- `grp_9_stash_abort_requires_confirmation`: Cancel/no write and confirmed job.
- `ops_7_conflicting_pop_offer_follows_resolution`: Keep/Drop still use the
  stable retained stash and Drop's core confirmation after refresh.
- `ui_22_resolve_two_files_and_continue_using_only_the_keyboard`.

Set repo-local Git identity and `core.autocrlf=false`. Use JobEnded as the
barrier before fixture mutation, and the `act_until` approach in
`tests/integration/stage.rs` when a test's own Git command wakes a watcher
whose refresh can overtake the next request. Enlarge windows for offscreen
file rows and controls. No timing assertions on hosted runners.

## Out of scope
- AI settings, consent, eligibility, proposal jobs and Resolve all: M6-23;
  M6-19b's supplied-proposal review remains ready for that wiring.
- Group/preflight/report UI: M6-21/22; palette registration: M6-24; full
  accessibility pass: M6-25.
- New conflict algorithms, core policy, spec/ADR changes or dependency upgrades.

## Acceptance
- All named tests and `just check` pass on the final implementation. Review
  all three M6-19 slices against OPS-2 through OPS-9 and the stated boundaries;
  existing legacy tests alone do not prove the native UI works.
- On the reference machine, resolve a two-file merge (one take and one manual
  edit), switch repo/tab and back, Mark resolved and Continue. Repeat with
  Abort, an edited-outside file, a choice conflict and a conflicting stash pop.
  Verify right/bottom docking and keyboard focus.
- Carry forward M6-19b's IME, visual and performance evidence. Report any
  outstanding checks explicitly; do not claim they were performed by CI.
- Follow DEV-4: split further if needed rather than exceeding about 400
  changed non-test lines in one PR.
