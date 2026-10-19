# M5-02 Requirement coverage over the whole spec

## Goal
`just coverage spec` lists every requirement ID defined anywhere in `docs/spec/` that has no test, so M5 can prove that every requirement ID has one (section 13, QA-3). The process rules DEV-1 to DEV-4 are recorded as needing no test.

## Depends on
M4

## Spec
Implements: QA-3 (over the whole spec), DEV-1, DEV-2, DEV-3, DEV-4
No test needed: DEV-1 (a rule for how spec changes are committed, kept by review), DEV-2 (a rule for how an agent works, kept by review), DEV-3 (a rule for what an agent may edit, kept by review), DEV-4 (a rule for how tasks are cut, kept when task files are written)
Respects: QA-1, QA-5, ADR 0001

## In scope
- `req-coverage spec` (and `just coverage spec`), beside the existing milestone mode:
  - The IDs are read from `docs/spec/*.md`: every bold definition `**ID**` (for example `- **ARCH-1**` and `* **UI-34**`), and every row of the decision log whose first cell is `D<n>`. A bold ID that only refers to another one does not exist in the spec today; if one appears, it counts once.
  - An ID is *covered* when a test mentions it, by the rule the milestone mode already uses (`mentions`, `test_code`, the same directories).
  - Otherwise it is *no test needed* when some task file lists it on a "No test needed:" line, with that task and reason; otherwise it is *missing*.
  - Output as in the milestone mode: `no test needed:` lines, `missing:` lines, then `spec: n covered, n no test needed, n missing`. Exit 1 when anything is missing, unless `--report-only`.
- `just check-rust` runs `just coverage spec --report-only` after the milestone lines, so the list shows in every run. M5-20 makes it enforcing.
- Tests in `tools/req-coverage` (unit tests, fixture text in code, QA-2):
  - `qa_3_spec_mode_reads_bold_definitions_and_decision_rows`: both list forms and a decision-log table; bold words that are not IDs (`**Mark resolved**`) are ignored.
  - `qa_3_spec_mode_reports_missing_and_covered`.
  - `qa_3_spec_mode_takes_no_test_needed_from_any_task`, with the task named in the output.
  - `qa_3_spec_mode_counts_an_id_once`.
  - The existing milestone-mode tests keep passing unchanged.
- This task may edit `docs/adr/` (DEV-3): `docs/adr/0003-dev-rules-need-no-test.md`. Context: section 13 asks that every requirement ID have a test, and DEV-1 to DEV-4 are rules for the people and agents who work on the project, not behaviour of the app. Decision: they are listed under "No test needed" with a reason, in this task file; every other ID, the decision-log rows included, needs a test. Consequences: `just coverage spec` reports them as *no test needed*, never as missing.
- The pull request lists what `just coverage spec` reports at this commit. Expected missing, each with the M5 task that adds its test: D1, D2, D3, D5, D7, D8, D13 (M5-03), D15 (M5-06), PLAT-4 and D12 (M5-12), SEC-7 (M5-13). Any other missing ID is reported to Eric before this task is called done (DEV-2).

## Out of scope
- Writing the missing tests (M5-03 onward).
- Making the spec mode enforcing (M5-20).

## Acceptance
- The tests above exist and pass, `just check` passes and prints the spec-wide list, and ADR 0003 exists.
