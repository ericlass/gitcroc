# M0-01 Workspace skeleton

## Goal
The repository builds as one Cargo workspace containing the five crates of the spec, with the lint rules that every later task relies on.

## Depends on
Nothing.

## Spec
Implements: ARCH-5, QA-4
Respects: D1, D14, section 4 "Crates" table

## In scope
- Root `Cargo.toml` workspace with members `crates/gitcroc-git`, `crates/gitcroc-core`, `crates/gitcroc-ai`, `crates/gitcroc-app`, `crates/gitcroc-cli`. Edition 2024, `license = "MIT"`, shared `[workspace.package]` and `[workspace.dependencies]`.
- `rust-toolchain.toml` pinning one stable version, with `rustfmt` and `clippy` components.
- Each crate compiles with a minimal `lib.rs` (or `main.rs` for `gitcroc-cli`). `gitcroc-app` is a plain library crate for now; Tauri arrives in M0-04.
- Internal dependencies exactly as the Crates table allows: core depends on git; cli depends on core and ai; app depends on git, core and ai; git and ai depend on nothing internal.
- Workspace lints (QA-4): `unsafe_code = "forbid"`; clippy `unwrap_used` and `expect_used` denied outside tests (allow them under `cfg(test)`).
- One typed error enum per library crate using `thiserror`, even if it has a single variant.
- `gitcroc-cli`: a `--version` flag that prints the workspace version. Nothing else.
- `.gitignore` for Rust, Node and Tauri build output.
- Test `arch_5_core_crates_do_not_depend_on_tauri`: reads `cargo metadata` and fails if `gitcroc-git`, `gitcroc-core` or `gitcroc-ai` has any crate named `tauri*` or `wry` in its dependency tree.
- Test `arch_5_internal_dependencies_match_spec`: fails if any internal dependency edge is not in the allowed list above.
- Test `qa_4_workspace_lints_forbid_unsafe_and_unwrap`: fails unless the workspace `Cargo.toml` sets the QA-4 lints above, `clippy.toml` allows `unwrap` and `expect` in tests, and every crate sets `[lints] workspace = true`. It lives under `crates/` so the requirement coverage script (M0-02) finds it.

## Out of scope
- Any git, AI, Tauri or UI code.
- `justfile`, cargo-deny, CI (M0-02, M0-03).

## Acceptance
- `cargo build --workspace` and `cargo test --workspace` pass.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- Adding `unsafe {}` or `.unwrap()` to non-test code in any crate makes the build or clippy fail (verify once by hand, then revert).
- Both `arch_5_*` tests exist and pass.
