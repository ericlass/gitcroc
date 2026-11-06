# Fails if a requirement of the current milestone has no test (QA-3).
# Everything CI requires (QA-1, QA-5); must pass before a task is done.
check: check-rust a11y-where-possible

# What CI's check-rust job runs: all of `just check`, with the requirement
# coverage.
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
    just coverage M5
    just coverage M6
    just coverage spec

# The accessibility tree of the real window, view by view (M6-25a): Linux
# only, in its own Xvfb server, D-Bus session and AT-SPI bus, so the
# desktop's own screen reader and settings are untouched. Needs xvfb-run,
# dbus-run-session and at-spi2-core. Each view's tree is written to
# target/tmp/a11y_tree.
a11y:
    cargo test -p gitcroc-ui --test a11y_tree --no-run
    crates/gitcroc-ui/tests/a11y_tree/session.sh cargo test -p gitcroc-ui --test a11y_tree

# `just a11y` where it can run: on Linux with xvfb-run. CI runs it as a
# step of its own.
a11y-where-possible:
    {{ if os() == "linux" { "if command -v xvfb-run >/dev/null; then just a11y; else echo 'a11y: skipped; it needs xvfb-run'; fi" } else { "echo 'a11y: skipped; it runs on Linux only'" } }}

# The app, `gitcroc`, in a debug build. It reads and writes the same
# settings, workspaces and cache as an installed gitcroc. Do not run both at
# once: each keeps its own copy of that state and writes it back.
dev:
    cargo run -p gitcroc-ui --bin gitcroc

# Format the workspace.
fmt:
    cargo fmt

# List the requirement IDs of a milestone (e.g. M0), or with `spec` of the
# whole spec, that have no test (QA-3).
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

# Build a reference data set of spec section 9: target-project, big-tree,
# many-repos, big-history or all. Data goes to target/bench-data, or to
# $GITCROC_BENCH_DATA when set. A set that is up to date is not rebuilt.
bench-data set:
    cargo run --quiet --release -p repo-gen -- {{set}}

# Performance harness (PERF-8) on target-project; `--stretch` adds the stretch
# sets that exist locally. Prints JSON.
bench *flags:
    cargo run --quiet --release -p gitcroc-cli -- bench {{flags}}

# UI-side budgets of section 9 (PERF-8) on the GPUI app and target-project,
# Linux only. Builds `gitcroc` with the `perf-trace` feature into its own
# target dir, never where a release builds, and runs it through
# `gitcroc-cli bench-ui`, which prints JSON like `just bench`. Run by hand
# before a release, pinned to 8 cores, with nothing else busy and the window
# left in front: `taskset -c 0-7 just bench-ui`.
bench-ui *flags:
    CARGO_TARGET_DIR="{{justfile_directory()}}/target/bench-ui" cargo build --quiet --release -p gitcroc-ui --bin gitcroc --features perf-trace
    cargo run --quiet --release -p gitcroc-cli -- bench-ui --app "{{justfile_directory()}}/target/bench-ui/release/gitcroc" {{flags}}

# Section 12: real-window smoke, isolated fixtures, Linux and Windows.
smoke:
    cargo build -p gitcroc-cli
    CARGO_TARGET_DIR="{{justfile_directory()}}/target/e2e" cargo test -p gitcroc-ui --bin gitcroc --features e2e qa_3_smoke
    CARGO_TARGET_DIR="{{justfile_directory()}}/target/e2e" cargo build -p gitcroc-ui --features e2e
    python .github/scripts/smoke.py --app "{{justfile_directory()}}/target/e2e/debug/gitcroc{{if os() == "windows" { ".exe" } else { "" }}}" --cli "{{justfile_directory()}}/target/debug/gitcroc-cli{{if os() == "windows" { ".exe" } else { "" }}}"
