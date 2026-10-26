# 15. Open questions

All ten questions are answered. The answers below are summaries; the decision log and the cited rules are the binding text.

- [x] **Name.** gitcroc. Still to do before the first public commit: check it is free on GitHub, crates.io and as a domain.
- [x] **License (D14).** MIT.
- [x] **Front-end framework (D2).** GPUI, chosen for performance after a spike (ADR 0004).
- [x] **Bundling git (D13).** No. System git is required on every platform, including Windows.
- [x] **Code signing.** Windows and macOS ship unsigned for now (SEC-7).
- [x] **Interactive rebase.** Not in v1 (section 2).
- [x] **Group commit.** In v1 (GRP-15, UI-34).
- [x] **Scale numbers (D5).** The first target is about 20 repos, 200,000 files and 2 million lines of code. Larger sizes are stretch targets (section 9).
- [x] **Stash on checkout.** No automatic stashing; the user stashes manually (GRP-12).
- [x] **App identifier.** `dev.gitcroc.desktop`, which replaces `dev.gitcroc.app` before the first release (ADR 0002).

## Sources

- [gitoxide crate status](https://github.com/GitoxideLabs/gitoxide/blob/main/crate-status.md): push, merge and rebase still unfinished
- [Tauri releases](https://tauri.app/release/tauri/): 2.11.5 is the latest release listed, July 2026
- [Claude Code headless mode](https://docs.claude.com/en/docs/claude-code/headless): `-p` with `--output-format json`
