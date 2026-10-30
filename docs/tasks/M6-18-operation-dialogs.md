# M6-18 Sync, integrate, stash, tag, reset and remote dialogs

## Goal
Port rc.1's single-repo operation surfaces to GPUI, enabling the M6-17 menu entries and the toolbar. Core services remain the authority.

## Spec
D15; ARCH-1, ARCH-3, ARCH-6, ARCH-7, ARCH-9; GRP-5, GRP-8, GRP-9, GRP-12; section 5 operation options; section 6 operations; OPS-7; UI-7, UI-13, UI-20; SEC-3, SEC-4; QA-2, QA-3, QA-5.

## Slices
1. Sync and integrate: Fetch/Pull/Push default clicks and options, Branch, Merge/rebase, history cherry-pick/revert and merge-parent choice. Core-filtered refs, push choices, explicit force confirmation.
2. Stash: optional message/untracked, apply/pop/drop by stable hash, drop confirmation, retained conflicting-pop offer once refreshed and out of conflict.
3. Tags/reset/remotes: validated names; annotated/lightweight tags and optional push; remote selection and destructive confirmations; soft/mixed/hard reset; add/rename/edit/remove remotes with loaded fetch/push URLs.

Port ui/src/layout/Toolbar.tsx, sync/SyncDialogs.tsx, integrate/IntegrateDialogs.tsx, stash/StashDialogs.tsx, tag/TagDialogs.tsx, reset/ResetDialogs.tsx, remote/RemoteDialogs.tsx and their stores. Reuse dialog.rs and the window's open_dialog pattern. Externalize strings. Keep errors inline, disable duplicate submissions, restore focus, use Cancel as confirmation's first focus. Enter in a field cannot confirm a destructive request.

## Boundaries
Group operations/preflight/report are M6-21/22; group and multi-repo toolbar actions remain visibly disabled with a reason. Conflict banner/editor and Abort/Continue entry points are M6-19. Palette is M6-24; show its disabled trigger until then. Settings is M6-26; its trigger also remains visibly disabled. No core operation reimplementation.

## Acceptance
Requirement-named tests written before production code exercise real git through GPUI: default/options sync and remote choice, merge/rebase and merge-parent revert, cherry-pick multiple commits, stash lifecycle, tag types/remote selection, reset modes and Cancel/confirmation safety, remote lifecycle/URLs, typed validation, disabled toolbar targets, literal repo text and inline refusals. JobEnded is the fixture mutation barrier. Fixture text comparisons disable core.autocrlf. Enlarge test windows where needed. just check passes. Reference-machine visual smoke remains separately reported if unavailable.

## Validation
- 17 GPUI flows against real fixture repos and two retained-stash model tests pass. The existing menu presentation tests now require the M6-18 actions to be enabled.
- `just check` passes, including the full workspace tests, linting, dependency checks and spec coverage (127 covered, zero missing).
- The native binary presents a real frame and exits successfully with `--exit-after-first-frame` under isolated Xvfb. Interactive reference-machine visual smoke has not been performed.
