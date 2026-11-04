# M6-28b The release workflow without Tauri

## Goal
Pushing a version tag builds the GPUI packages, signs the update packages with the project's updater key, and prepares a draft GitHub release, as 1.0.0-rc.1's workflow did. The draft has the files, `SHA256SUMS`, the release notes and the `latest.json` the updater reads.

The signatures are made by our own step, in Tauri's format with the `version:` field, so both rc.1's Tauri updater and `OwnSource` accept them. Eric reviews and publishes the draft. Nothing is published automatically.

This is the second of M6-28's three pull requests.

## Depends on
M6-28a

## Spec
Implements: SEC-7 (again, without Tauri)
Respects: PLAT-4, D6, D12, D14, QA-1, section 11 platform table

## In scope
- **A `sign` command in `tools/release-manifest`.** M6-03 found that cargo-packager's signer leaves out `version:`, so it isn't used.
  - **Usage:** `release-manifest sign --version <v> <file>...` writes `<file>.sig` next to each file.
  - **The key:** read from the environment variables `TAURI_SIGNING_PRIVATE_KEY` (base64 of a minisign secret key box) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. These are the same secrets as rc.1's (M5-12), and they keep their names.
  - **The `.sig`:** base64 of a minisign signature box, with the trusted comment `timestamp:<s>\tfile:<name>\tversion:<v>`, as the Tauri CLI writes it.
  - **Crate:** it uses the `minisign` crate already in the workspace.
  - **Errors:** exit 1 with no key, a wrong password or a missing file; exit 2 on a usage error.
- **`.github/workflows/release.yml`.** Rc.1's, on the `v1.0.0-rc.1` tag, is the starting point.
  - **When it runs:** on a pushed tag `v*`. A dry run, which uploads the release as a workflow artifact and creates no release, runs by hand and on pull requests that change the workflow, `packages.yml` or `tools/release-manifest`.
  - **The tag:** the workflow fails at once unless the tag is `v` plus the workspace version (`one_version_number`).
  - **The packages:** built by calling M6-28a's `packages.yml`, with the update archives, with no build steps of its own. Its install jobs run too, so a release is only drafted from packages that installed.
  - **Signing:** a separate job signs the NSIS installer, `gitcroc.app.tar.gz` and the AppImage with `release-manifest sign`.
    - The secrets appear only as environment variables of that step.
    - A dry run signs with a key it generates for the run, and its `latest.json` must never be published. Its summary says so.
  - **Assembling:** `SHA256SUMS`, then `release-manifest assemble` for `latest.json` and the notes. These are unchanged from rc.1: the version's changelog section, then the download paragraph.
  - **The draft:** named `gitcroc <version>`, with every file. It is a pre-release when the version has a pre-release part. Only the job that creates it has `contents: write`.
- **`RELEASING.md`:**
  - **The M6 note:** it goes, since releases from `main` work again.
  - **The steps:** the Tauri-specific parts change. `ui/package.json` is no longer bumped and `tauri.release.conf.json` is no longer named. The file list stays.
  - **A new step:** before publishing, download `latest.json` and one `.sig` from the draft, and check that `minisign -V` with the project's public key accepts the package. The workflow also checks this itself (below).
- **The workflow checks its own output.** After signing, a tag build verifies each signature with the project's public key, so a wrong secret fails the run before the draft is created. A dry run verifies with the run's own key.
  - **The command:** `release-manifest verify --version <v> [--public-key <base64>] <file>...`. Without `--public-key` it uses the project's key.
  - **How it verifies:** with `minisign-verify`, as `OwnSource` does, including the `version:` check.
  - **The key:** `release-manifest` holds its own copy of the public key. `release-manifest` can't depend on `gitcroc-app`, which already dev-depends on it, so a test in `gitcroc-app` checks the copy equals `verify::PUBLIC_KEY`.
- **Tests:**
  - `sec_7_signature_carries_the_version`: a file signed by `sign` verifies with `gitcroc_app::update::verify::verify` for its version, and is refused for another version.
  - `sec_7_sign_reads_tauris_secret_key_format`: a key made by `minisign`, written out the way `tauri signer generate` does (base64 of the box, with a password), signs; a wrong password is an error.
  - `sec_7_release_manifest_verifies_with_the_apps_key`: in `gitcroc-app`, `release_manifest`'s public key equals `verify::PUBLIC_KEY`.
  - `sec_7_tag_builds_verify_with_the_public_key`: in `release.yml`, the verify step runs on tag builds without `--public-key`.
  - `sec_7_only_the_signing_step_sees_the_key`: in `release.yml`, `TAURI_SIGNING_PRIVATE_KEY` appears only in the signing step's `env`.
  - The existing `tools/release-manifest` tests still pass.

## Out of scope
- **Pushing a tag, or creating and publishing a real release:** that is M6-31, by Eric. An agent never does either.
- **The update tests on the runners:** M6-28c.
- **Code signing and notarization:** SEC-7, LATER.

## Acceptance
- The pull request's own dry run is green, lists the files a release would get, and creates no release. It is linked in the pull request.
- The tests above pass, and `just check` passes.
