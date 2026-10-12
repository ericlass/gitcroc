# Everything CI requires (QA-1, QA-5); must pass before a task is done.
# Coverage is report-only until M0-08 makes it enforcing.
check:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo deny check
    pnpm --dir ui install --frozen-lockfile
    pnpm --dir ui exec tsc --noEmit
    pnpm --dir ui exec eslint .
    pnpm --dir ui exec vitest run
    pnpm --dir ui exec playwright test
    just coverage M0 --report-only

# One-time setup after cloning: front-end packages and Playwright's Chromium.
setup:
    pnpm --dir ui install --frozen-lockfile
    pnpm --dir ui exec playwright install chromium

# Format the workspace.
fmt:
    cargo fmt

# List the requirement IDs of a milestone (e.g. M0) that have no test (QA-3).
coverage milestone *flags:
    cargo run --quiet -p req-coverage -- {{milestone}} {{flags}}

# Run the app.
dev:
    TAURI_APP_PATH="{{justfile_directory()}}/crates/gitcroc-app" pnpm --dir ui exec tauri dev

# Performance harness.
bench:
    @echo "not available until M1" >&2; exit 1
