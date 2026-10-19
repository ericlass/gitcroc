# CI runs check-rust and check-ui as separate jobs, in parallel.
# Fails if a requirement of the current milestone has no test (QA-3).
# Everything CI requires (QA-1, QA-5); must pass before a task is done.
check: check-rust check-ui

# The Rust half of `just check`, with the requirement coverage.
check-rust:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo deny check
    just coverage M0
    just coverage M1
    just coverage M2
    just coverage M3
    just coverage M4

# The front-end half of `just check`.
check-ui:
    pnpm --dir ui install --frozen-lockfile
    pnpm --dir ui exec tsc --noEmit
    pnpm --dir ui exec eslint .
    pnpm --dir ui exec vitest run
    pnpm --dir ui exec playwright test

# One-time setup after cloning: front-end packages and Playwright's Chromium.
setup:
    pnpm --dir ui install --frozen-lockfile
    pnpm --dir ui exec playwright install chromium

# Regenerate the TypeScript IPC bindings (ui/src/ipc/bindings.ts) from the
# Rust commands and events. `just check` fails while they are out of date.
ipc:
    cargo run --quiet -p gitcroc-app --example ipc_bindings

# Format the workspace.
fmt:
    cargo fmt

# List the requirement IDs of a milestone (e.g. M0) that have no test (QA-3).
coverage milestone *flags:
    cargo run --quiet -p req-coverage -- {{milestone}} {{flags}}

# Tests that assert times. Never on hosted CI runners, whose speed varies;
# run them on the reference machine.
timing:
    cargo test --workspace -- --ignored

# Run the prompt fixture set (AI-15) against a real provider and score each
# hunk, for example `just ai-eval --provider p1 --key-env ANTHROPIC_API_KEY`
# or `just ai-eval --kind claude_code --path /usr/local/bin/claude --compare
# v1,v2`. The key is read only from the named variable (SEC-1). Never part of
# `just check` or CI.
ai-eval *args:
    cargo run --quiet -p gitcroc-cli -- ai eval {{args}}

# Run the app.
dev:
    TAURI_APP_PATH="{{justfile_directory()}}/crates/gitcroc-app" pnpm --dir ui exec tauri dev

# Build the installable bundles, for example `just bundle --bundles deb`.
# CI runs this in .github/workflows/build.yml.
bundle *flags:
    TAURI_APP_PATH="{{justfile_directory()}}/crates/gitcroc-app" pnpm --dir ui exec tauri build {{flags}}

# Build a reference data set of spec section 9: target-project, big-tree,
# many-repos, big-history or all. Data goes to target/bench-data, or to
# $GITCROC_BENCH_DATA when set. A set that is up to date is not rebuilt.
bench-data set:
    cargo run --quiet --release -p repo-gen -- {{set}}

# Performance harness (PERF-8) on target-project; `--stretch` adds the stretch
# sets that exist locally. Prints JSON.
bench *flags:
    cargo run --quiet --release -p gitcroc-cli -- bench {{flags}}

# End-to-end smoke test (section 12, M2-15) on the real app, Linux only:
# add a repo, stage, commit, and resolve a merge conflict by hand; then the
# group flow (M3-15) and the AI flow with a mock provider (M4-12). Builds the
# app with the `e2e` feature into its own target dir and drives it through
# tauri-driver and WebKitWebDriver. CI's Linux job runs it under xvfb-run.
smoke:
    CARGO_TARGET_DIR="{{justfile_directory()}}/target/e2e" TAURI_APP_PATH="{{justfile_directory()}}/crates/gitcroc-app" pnpm --dir ui exec tauri build --debug --no-bundle --features e2e >&2
    CARGO_TARGET_DIR="{{justfile_directory()}}/target/e2e" cargo build --quiet -p gitcroc-cli
    node ui/webdriver/smoke.ts

# UI-side budgets of section 9 (PERF-8) on the real app and target-project,
# Linux only. Builds the app with the `perf-trace` feature into its own target
# dir, never where `just bundle` builds, and drives it through tauri-driver
# and WebKitWebDriver. Prints JSON like `just bench`. Run by hand before a
# release, pinned to 8 cores: `taskset -c 0-7 just bench-ui`.
bench-ui *flags:
    CARGO_TARGET_DIR="{{justfile_directory()}}/target/bench-ui" TAURI_APP_PATH="{{justfile_directory()}}/crates/gitcroc-app" pnpm --dir ui exec tauri build --no-bundle --features perf-trace --config "{{justfile_directory()}}/crates/gitcroc-app/tauri.perf.conf.json" >&2
    cargo build --quiet --release -p gitcroc-cli
    node ui/bench/bench-ui.ts {{flags}}
