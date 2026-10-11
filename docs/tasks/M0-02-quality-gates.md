# M0-02 Quality gates

## Goal
One command, `just check`, runs every Rust check that CI requires, and a script reports spec requirements that have no test.

## Depends on
M0-01

## Spec
Implements: QA-3, QA-5
Respects: QA-1, QA-2, D14

## In scope
- `justfile` with:
  - `just check`: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check`, then `just coverage M0`.
  - `just fmt`: formats the workspace.
  - `just coverage <milestone>`: runs the requirement coverage script.
  - `just dev` and `just bench` as recipes that print "not available until M0-04" and "not available until M1" and exit non-zero.
- `deny.toml` for cargo-deny: license allowlist compatible with MIT (MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, MPL-2.0 for dependencies only); advisories and bans enabled; unknown registries and git sources denied.
- Requirement coverage script at `tools/req-coverage` (Rust binary in the workspace, not published, or a script with no dependencies beyond the toolchain already required):
  - Input: a milestone prefix such as `M0`.
  - Collects every ID on an "Implements:" line in `docs/tasks/<prefix>-*.md`.
  - Searches test code in `crates/` and `ui/` for each ID, matching both forms: `GRP-6` and `grp_6`.
  - Prints the IDs with no match and exits non-zero if there are any.
  - A task file can mark an ID as proven without a test by listing it on a "No test needed:" line with a reason; the script then skips it and prints the reason.
- Unit tests for the script: ID extraction from a task file, both name forms matched, missing ID reported.
- `CONTRIBUTING.md`, short: how to run `just check`, the test naming rule (QA-3), and that fixture repos are built in code, never checked in as archives (QA-2).
- Update `CLAUDE.md`: remove the sentence saying the commands do not exist yet for `just check`; keep it for `dev` and `bench`.

## Out of scope
- Front-end checks (`tsc`, ESLint, Vitest, Playwright). M0-04 adds them to `just check`.
- CI configuration (M0-03).

## Acceptance
- `just check` passes on a clean checkout and fails when a formatting error, a clippy warning, a failing test or a denied license is introduced (verify each once by hand).
- `just coverage M0` lists the IDs from later M0 tasks as missing at this point; that is expected, so `just check` in this task calls it in report-only mode. M0-08 switches it to enforcing.
- Tests named `qa_3_*` and `qa_5_*` exist and pass (`qa_5`: a test that parses the `justfile` and asserts the `check` recipe contains each command QA-1 lists for Rust).
