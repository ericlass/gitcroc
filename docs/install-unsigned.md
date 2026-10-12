# Installing the unsigned builds

gitcroc's Windows and macOS builds are not code-signed or notarized yet (SEC-7). The first time you open one, the operating system warns that the developer cannot be verified. The steps below get past that warning once; after that the app opens normally.

Only do this for files you downloaded from gitcroc's own GitHub releases or CI artifacts.

## Windows

Running the installer (`gitcroc_<version>_x64-setup.exe`) or `gitcroc.exe` from the portable zip shows a blue Microsoft Defender SmartScreen window, "Windows protected your PC".

1. Click **More info**.
2. Click **Run anyway**.

The installer installs Microsoft Edge WebView2 if it is missing. The portable zip does not; it needs WebView2 to be present already, which it is on current Windows 10 and 11.

## macOS

The `.dmg` holds a universal app for Apple silicon and Intel Macs, macOS 12 or newer. The app is ad-hoc signed, which Apple silicon requires, but not notarized, so Gatekeeper refuses to open it with a double-click and says Apple cannot check it for malicious software.

Drag gitcroc to Applications, then do one of these:

- **Right-click, Open.** In Finder, right-click (or Control-click) gitcroc in Applications and choose **Open**, then **Open** again in the dialog. On macOS 15 and newer this dialog may only offer **Done**; then open **System Settings > Privacy & Security**, scroll down to the message about gitcroc and click **Open Anyway**.
- **Remove the quarantine attribute.** In Terminal:

  ```sh
  xattr -d com.apple.quarantine /Applications/gitcroc.app
  ```

## Linux

The AppImage, `.deb` and `.rpm` packages show no such warning. Make the AppImage executable before running it (`chmod +x gitcroc_<version>_amd64.AppImage`).
