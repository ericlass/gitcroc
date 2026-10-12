# Everything CI requires for Rust (QA-1, QA-5); must pass before a task is done.
# Coverage is report-only until M0-08 makes it enforcing.
check:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo deny check
    just coverage M0 --report-only

# Format the workspace.
fmt:
    cargo fmt

# List the requirement IDs of a milestone (e.g. M0) that have no test (QA-3).
coverage milestone *flags:
    cargo run --quiet -p req-coverage -- {{milestone}} {{flags}}

# Run the app.
dev:
    @echo "not available until M0-04" >&2; exit 1

# Performance harness.
bench:
    @echo "not available until M1" >&2; exit 1
