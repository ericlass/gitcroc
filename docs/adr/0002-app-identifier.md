# ADR 0002: The app identifier is dev.gitcroc.desktop

- Status: accepted
- Date: 2026-10-19
- Concerns: section 15 "App identifier", section 10 "Files", D6, tasks M0-04 and M5-01

## Context

Task M0-04 gave the app the identifier `dev.gitcroc.app` in
`crates/gitcroc-app/tauri.conf.json`. Tauri advises against an identifier
ending in `.app`, because on macOS it looks like the application bundle
extension. The builds work with it, so section 15 kept the question open.

The operating systems key data to the identifier: the macOS bundle id, and on
every platform the folder that holds the webview's own storage. Changing the
identifier after a release would leave that data behind under the old name for
every user. No release has shipped yet, so a change now costs nothing but the
developer's own webview storage.

gitcroc's own data does not depend on the identifier. The config, cache and log
directories are named `gitcroc` (M0-07), and keychain entries use the service
`gitcroc` and the account `ai-provider-<id>` (M4-05).

## Decision

The app identifier is `dev.gitcroc.desktop`, set in
`crates/gitcroc-app/tauri.conf.json`. No overlay config sets another one. The
test `the_identifier_is_dev_gitcroc_desktop` in
`crates/gitcroc-app/tests/integration/build.rs` checks both.

## Consequences

- Section 15's last open question is answered; all ten are.
- The webview's storage moves to a folder under the new name. It holds no
  source of truth (ARCH-6), so it starts empty once on the developer's machine
  and loses nothing.
- Settings, workspaces, `cache.db`, logs and keychain entries stay where they
  are; nothing is migrated.
- A later change of the identifier would cost user data, so it needs a new ADR
  and a migration.
