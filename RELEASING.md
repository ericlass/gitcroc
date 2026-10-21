# Releasing gitcroc

A release is a pushed tag. The Release workflow (`.github/workflows/release.yml`) builds every package, signs the update packages and prepares a **draft** release. Nothing is published until you publish the draft.

Pushing tags and publishing releases is Eric's job. An agent never does either; it prepares the version bump and the changelog in a pull request.

## Before the first release

The repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` hold the updater's private key and its password (M5-12). Only a tag build reads them, in the step that builds the bundles. A dry run signs with a key it makes for the run, so its `latest.json` does not verify against the app's public key and must never be published.

## Steps

1. **Bump the version** in the workspace `Cargo.toml` (`[workspace.package] version`), and in `ui/package.json` if it repeats it; `one_version_number` fails when they disagree. A pre-release has a pre-release part, for example `1.0.0-rc.1`.
2. **Write the changelog section.** In `CHANGELOG.md`, move what is under `Unreleased` into a new section `## [<version>] - <YYYY-MM-DD>` and leave `Unreleased` empty. The section becomes the release notes and the notes the update dialog shows, so write it for users. The workflow fails when the version has no section.
3. **Merge** the pull request with both changes into `main`, and wait for CI to be green.
4. **Tag and push** the merge commit:

   ```sh
   git switch main && git pull
   git tag -a v<version> -m "gitcroc <version>"
   git push origin v<version>
   ```

   The tag must be `v` plus the version in `Cargo.toml`; otherwise the workflow stops before building anything.
5. **Watch the workflow** (Actions, Release). The run's summary lists every file, `latest.json` and the notes.
6. **Check the draft** under Releases:
   - **Files:** `gitcroc_<version>_x64-setup.exe` and its `.sig`, `gitcroc-<version>-windows-x64-portable.zip`, `gitcroc_<version>_universal.dmg`, `gitcroc.app.tar.gz` and its `.sig`, `gitcroc_<version>_amd64.AppImage` and its `.sig`, `gitcroc_<version>_amd64.deb`, `gitcroc-<version>-1.x86_64.rpm`, `SHA256SUMS` and `latest.json`.
   - **`latest.json`:** `version` is the new version. The four platforms `windows-x86_64`, `darwin-x86_64`, `darwin-aarch64` and `linux-x86_64` point to `https://github.com/ericlass/gitcroc/releases/download/v<version>/…` and each has a signature. `pub_date` is when the workflow ran.
   - **Notes:** the changelog section, then the download paragraph with the link to `docs/user/install.md`.
   - **Pre-release:** the box is ticked exactly when the version has a pre-release part.
7. **Publish** the draft. Publish a pre-release as a pre-release. GitHub's `releases/latest`, where the updater looks for `latest.json`, ignores pre-releases and drafts, so installed apps are offered final versions only.

## If something is wrong

- **Before publishing:** delete the draft and the tag (`git push origin :refs/tags/v<version>`, then `git tag -d v<version>`), fix it in a pull request, and tag again.
- **After publishing:** do not move the tag. Release a new patch version instead. Apps that already updated keep the published version.

## Dry runs

Actions, Release, "Run workflow" builds and signs everything from the chosen branch with a key made for the run, and uploads the release as the artifact `gitcroc-<version>-release`, without creating a release. A pull request that changes the release workflow, `bundles.yml`, `tauri.release.conf.json` or `tools/release-manifest` runs one too. When the version has no changelog section yet, a dry run takes the `Unreleased` section, or the newest section when `Unreleased` is empty (right after a release), and says so in its log.
