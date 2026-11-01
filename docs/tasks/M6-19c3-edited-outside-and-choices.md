# M6-19c3 Edited outside, and choice conflicts

Third implementation PR of [M6-19c](M6-19c-conflict-flow-and-jobs.md),
under DEV-4. Depends on [M6-19c2](M6-19c2-conflict-files-and-resolve.md).

Implements: OPS-6, OPS-8
Respects: OPS-5, OPS-9, ARCH-6, UI-22, SEC-3, D15, QA-2, QA-3, QA-4, QA-5

On `edited_outside`, ask Keep the file as it is or Start over from the
conflict before showing an editor. Keep opens the core's disk text as plain
editable text with no hunks or hunk actions, even when it looks like
markers; it is disabled with the reason when the core has no disk text.
Start over builds the editor from the core's chunks. Neither writes; the
answer stays with the file's kept state. Choice conflicts show the reason
and both core-labelled sides with what each holds (deleted, size and
executable, symbolic link, submodule commit), each with Use this version
calling `conflict_choose`, and no text editor.

Write first: `ops_8_edited_outside_requires_a_choice` and
`ops_6_choice_conflicts_show_and_use_core_versions`.

Acceptance: these tests and `just check` pass.
