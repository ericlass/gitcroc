# M0 sign-off

Milestone M0 (docs/spec/13-milestones.md) is done when an empty three-pane window opens on all three platforms from the CI artifacts and QA-1 is green. The person verifying fills in this file and commits it.

Get the artifacts from the latest green run of the **Build** workflow on `main` (`gitcroc-<version>-<platform>`). [docs/install-unsigned.md](../install-unsigned.md) explains how to get past the unsigned-build warnings.

Mark each check `[x]` when it passes and add notes after it: the OS warning you saw, or what failed.

The git version in Settings is the version number only; Git for Windows adds a suffix such as `.windows.1` to `git --version` that gitcroc does not show.

## Status

Deferred for Windows and macOS (decided 2026-10-12): no Windows PC or Mac is available to test on, so M0 proceeds on the assumption that those builds work, and these sections are filled in later. For the first Build run (38233068921) the artifacts were inspected without running them: the Windows installer is an NSIS installer and the portable zip holds a 64-bit `gitcroc.exe`; the macOS dmg holds an ad-hoc signed universal (x86_64 and arm64) app with minimum macOS 12.0; none of the binaries contains an updater.

## Run

- Build workflow run (URL):
- Version:
- CI workflow on the same commit is green (QA-1): [ ]
- Verified by, date:

## Windows (10 or 11, x64)

- OS version:
- [ ] The installer downloads and installs. OS warning shown:
- [ ] The portable zip unpacks and `gitcroc.exe` starts. OS warning shown:
- [ ] The window opens with the three-pane layout.
- [ ] The theme switch works and the theme survives a restart.
- [ ] With git present the app loads, and the git version in Settings matches `git --version`.

## macOS (Apple silicon required)

- OS version and chip:
- [ ] The dmg downloads and the app installs. OS warning shown:
- [ ] The window opens with the three-pane layout.
- [ ] The theme switch works and the theme survives a restart.
- [ ] With git present the app loads, and the git version in Settings matches `git --version`.

## macOS on Intel (best effort, section 9)

- OS version, or "not tested":
- [ ] The same dmg installs and the window opens with the three-pane layout.

## Linux (x86_64)

- Distribution and version:
- [x] The AppImage downloads and runs. (Build run 38233068921, 2026-10-12.)
- [ ] The `.deb` installs (Debian or Ubuntu), or the `.rpm` installs (Fedora or openSUSE). Which one:
- [ ] The window opens with the three-pane layout.
- [ ] The theme switch works and the theme survives a restart.
- [ ] With git present the app loads, and the git version in Settings matches `git --version`.
