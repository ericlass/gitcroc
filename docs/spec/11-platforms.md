# 11. Platforms, packaging and licensing

v1 ships installers for three platforms from one CI pipeline, with identical features on each.

| Platform | Supported versions | Packages | Graphics |
| --- | --- | --- | --- |
| Windows | 10 and 11, x64 | NSIS installer and portable zip, both unsigned in v1 (SEC-7) | Direct3D 11 |
| macOS | 12 and newer, universal binary | Not notarized in v1 (SEC-7); `.dmg` | Metal |
| Linux | x86_64, glibc 2.35 and newer | AppImage, `.deb`, `.rpm` | Vulkan, or GL where Vulkan is missing |

- **PLAT-1** No feature may be platform-specific. Platform differences (paths, keychain, askpass dialog, file manager, terminal) are isolated behind one trait per concern in `gitcroc-app`.
- **PLAT-2** Paths are handled as OS paths end to end (`PathBuf`), never as UTF-8 strings, and converted only for display. Windows long paths and case-insensitive file systems are covered by tests.
- **PLAT-3** On startup the app locates `git`, checks the version (D13), and records the path. On Windows it also looks in the standard Git for Windows install locations.
- **PLAT-4** Auto-update uses the app's own updater, which reads the same manifest and verifies with the same key as 1.0.0-rc.1, so that release updates to the GPUI build. It updates the Windows installer, the macOS app and the AppImage. `.deb` and `.rpm` only notify that a new version exists.

## License

The project is licensed under MIT (D14). Every dependency in section 4 is compatible with it, and `cargo deny` (QA-1) keeps it that way.
