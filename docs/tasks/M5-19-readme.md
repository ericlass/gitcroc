# M5-19 README as the project's front page

## Goal
The repository's `README.md` is gitcroc's front page and, with the GitHub Releases page, its download page: a visitor learns what gitcroc is, downloads the right file and finds the documentation (section 13 M5 "website or README"; README "Website and screenshots").

## Depends on
M5-18

## Spec
Respects: D4, D6, D12, D13, D14, SEC-7, section 2 goals and non-goals, section 11 platform table

## In scope
- `README.md`:
  - One paragraph on what gitcroc is and for whom: projects of many repositories, groups of repos operated on together, AI help with conflicts from the user's own account.
  - Features in a short list, from section 2's goals; what it does not do yet, in one line linking section 2's non-goals.
  - Download: a link to the latest release, and which file to take on Windows, macOS and Linux; the unsigned-build note with a link to `docs/user/install.md` (SEC-7); requirements (git 2.40 or newer, D13; the systems of D6).
  - Documentation: links to `docs/user/`.
  - Privacy: no telemetry, the three kinds of network call, and that the update check can be turned off (D12), linking `docs/user/privacy.md`.
  - Building from source and contributing: a link to `CONTRIBUTING.md`; the spec in `docs/spec/` as the source of truth.
  - License: MIT (D14).
  - The CI badge stays; the "early development" sentence goes.
- `CONTRIBUTING.md`: a "Releasing" line linking `RELEASING.md` (M5-13); nothing else changes.
- The repository description and website field on GitHub are Eric's to set; the pull request suggests text for both.

## Out of scope
- A website, screenshots, a logo change.

## Acceptance
- M5-18's link test covers the README and passes, and `just check` passes.
