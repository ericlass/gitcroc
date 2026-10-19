# ADR 0003: DEV-1 to DEV-4 need no test

- Status: accepted
- Date: 2026-10-19
- Concerns: QA-3, DEV-1, DEV-2, DEV-3, DEV-4, section 13, task M5-02

## Context

Section 13 asks that every requirement ID have a test by the end of M5 (QA-3).
Task M5-02 makes `just coverage spec` list every ID defined anywhere in
`docs/spec/` that no test mentions. DEV-1 to DEV-4 are rules for the people and
agents who work on the project: how spec changes are committed, when an agent
stops and asks, what an agent may edit and how tasks are cut. They describe no
behaviour of the app, so no test of the app can prove them.

## Decision

DEV-1 to DEV-4 are listed on the "No test needed:" line of
`docs/tasks/M5-02-spec-wide-coverage.md`, each with a reason. Every other ID,
the rows of the decision log included, needs a test.

## Consequences

- `just coverage spec` reports DEV-1 to DEV-4 as *no test needed*, never as
  missing.
- Review keeps the DEV rules: a pull request that breaks one is sent back.
- A new process rule added to section 14 needs its own line under "No test
  needed:" in the task that adds it, or a test.
