# Contributing to gitcroc

gitcroc is a Rust Cargo workspace. Crates live in `crates/`, and the future front end lives in `ui/`.

## Running checks

Run `just check` before you call a change done. It runs everything CI requires:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo deny check`
- `just coverage M0 --report-only` (the requirement coverage report; it lists missing IDs without failing until M0-08 makes it enforcing)

Run `just fmt` to format the workspace.

`just coverage <milestone>` (for example `just coverage M1`) lists requirement IDs from the `Implements:` lines of `docs/tasks/<milestone>-*.md` that have no test, and fails if there are any.

You need the Rust toolchain pinned in `rust-toolchain.toml`, `just`, and `cargo-deny`.

## Test naming (QA-3)

A test for requirement X carries X in its name or in a comment. For example, `grp_6_one_failure_does_not_stop_others` tests GRP-6. The coverage script accepts both `GRP-6` and `grp_6`. It searches test code under `crates/` and `ui/`, which means:

- Rust files under a `tests/` directory
- Rust code after a `#[cfg(test)]` attribute
- Front-end files named `*.test.*` or `*.spec.*`, or files under `tests/`, `e2e/` or `__tests__/`

A task file may list an ID on a `No test needed:` line with a reason in parentheses, for example `No test needed: QA-1 (the workflow itself is the proof)`.

## Fixture repos (QA-2)

Build fixture git repositories in code, with helper functions. Never check in repo archives (zip or tar) or other binary fixtures.

## Integration tests and unwrap (QA-4)

`unsafe` is forbidden. `unwrap` and `expect` are denied outside tests. `clippy.toml` allows them in tests, but clippy's `allow-unwrap-in-tests` does not cover helper functions in integration test files. So every integration test file (`tests/*.rs`) starts with:

```rust
#![allow(clippy::unwrap_used, clippy::expect_used)]
```

## Where things are

- Spec (source of truth): `docs/spec/`
- Decisions: `docs/spec/01-decisions.md` and `docs/adr/`
- Tasks: `docs/tasks/`. One task is one pull request.
