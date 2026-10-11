# 12. Testing and quality gates

Every requirement ID in this spec maps to at least one automated test that names the ID, so an agent can prove a requirement is met instead of claiming it.

| Level | Tool | What it covers |
| --- | --- | --- |
| Unit | `cargo test` | Parsers, graph lane layout, preflight classification, AI response validation |
| Git integration | `cargo test` with real `git` in temporary directories | Every operation in sections 5 and 6 against repos built by fixture helpers, including every *skipped*, *failed* and *conflict* path |
| Group scenarios | `gitcroc-cli` driven tests | Multi-repo flows such as the mixed-branch scenario in section 5, asserted on the result report |
| AI | Mock provider plus recorded responses | AI-8 to AI-10 validation, retry, splitting, cancellation. No test in CI calls a real provider |
| Front end unit | Vitest and Testing Library | Components against mocked IPC |
| Front end flows | Playwright against the Vite build with a fake core | Selection rules of section 8, themes, keyboard reachability (UI-22) |
| End to end smoke | WebDriver via `tauri-driver`, Linux and Windows | App starts, adds a repo, shows history, commits |
| Performance | Harness in `gitcroc-cli` (PERF-8) | Budgets of section 9 |

- **QA-1** CI runs on Windows, macOS and Linux for every pull request. Required to merge: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `cargo deny check`, `tsc --noEmit`, ESLint, Vitest, Playwright.
- **QA-2** Fixture repos are built in code by helper functions. No binary repo archives are checked in.
- **QA-3** A test for requirement X carries X in its name or a comment, for example `grp_6_one_failure_does_not_stop_others`. A script lists requirement IDs with no test; the list MUST be empty at each milestone for the requirements in that milestone.
- **QA-4** `unsafe` is forbidden in the project's crates. `unwrap` and `expect` are forbidden outside tests; errors are typed (`thiserror`) and carry git's stderr where there is one.
- **QA-5** One command, `just check`, runs everything QA-1 requires locally.
