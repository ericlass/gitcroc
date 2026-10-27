# Fails if a requirement of the current milestone has no test (QA-3).
# Everything CI requires (QA-1, QA-5); must pass before a task is done.
check: check-rust

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
    just coverage spec

# The old web front end (ui/), the reference the M6 tasks port from, in a
# browser with its fake core. It is not part of `just check`; M6-30 deletes
# it.
ui-reference:
    pnpm --dir ui install --frozen-lockfile
    pnpm --dir ui exec vite --mode fake

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
