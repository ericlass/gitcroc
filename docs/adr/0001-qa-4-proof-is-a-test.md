# ADR 0001: QA-4 is proven by a test under crates/

- Status: accepted
- Date: 2026-10-12
- Concerns: QA-3, QA-4, tasks M0-01 and M0-02

## Context

Task M0-01 said a comment naming QA-4 next to the lint block in the workspace
`Cargo.toml` was enough proof for QA-4. Task M0-02's requirement coverage script
searches only test code under `crates/` and `ui/`. It could never find that
comment, so QA-4 would be reported as untested once M0-08 makes the script
enforcing.

## Decision

QA-4 is proven by a test named `qa_4_*` under `crates/`, like every other
requirement (QA-3). The test is
`qa_4_workspace_lints_forbid_unsafe_and_unwrap` in
`crates/gitcroc-cli/tests/workspace_policy.rs`. It fails unless the workspace
`Cargo.toml` forbids `unsafe_code` and denies clippy `unwrap_used` and
`expect_used`, `clippy.toml` allows `unwrap` and `expect` in tests, and every
crate sets `[lints] workspace = true`.

The coverage script keeps its search scope, and QA-4 is not listed under
"No test needed:".

## Consequences

- The M0-01 task file now asks for this test instead of the comment.
- A new crate that forgets `[lints] workspace = true` fails the test instead of
  silently escaping QA-4.
- No spec edit is needed: QA-3 already requires a named test for every
  requirement, and this decision follows it.
