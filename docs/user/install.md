# Installing gitcroc

Download gitcroc from the [releases page](https://github.com/ericlass/gitcroc/releases/latest). Each release lists one file per package; `<version>` below stands for the version number, such as `1.0.0`. The release also has `SHA256SUMS`, with the checksum of every file, if you want to check your download.

| System | File | What it is |
| --- | --- | --- |
| Windows 10 and 11, x64 | `gitcroc_<version>_x64-setup.exe` | Installer. Updates itself |
| Windows 10 and 11, x64 | `gitcroc-<version>-windows-x64-portable.zip` | Portable: unzip and run, nothing is installed |
| macOS 12 and newer | `gitcroc_<version>_universal.dmg` | Disk image with one app for Apple silicon and Intel. Updates itself |
| Linux x86_64 | `gitcroc_<version>_amd64.AppImage` | Runs without installing. Updates itself |
| Linux x86_64 | `gitcroc_<version>_amd64.deb` | Package for Debian, Ubuntu and their relatives |
| Linux x86_64 | `gitcroc-<version>-1.x86_64.rpm` | Package for Fedora, RHEL, openSUSE and their relatives |

Whichever you choose, you also need git 2.40 or newer (see [What you need](README.md#what-you-need)).

The Windows and macOS builds are not code-signed or notarized, so the first time you open one, the system warns that the developer cannot be verified. The sections below show how to get past that warning once; after that, gitcroc opens normally. Only do this for files you downloaded from gitcroc's own releases page.

## Windows

**Installer.** Run `gitcroc_<version>_x64-setup.exe`. It installs gitcroc for your user account only, so it does not ask for administrator rights. gitcroc needs Microsoft Edge WebView2, which current Windows 10 and 11 already have; if it is missing, the installer downloads and installs it.

**Portable zip.** Unzip `gitcroc-<version>-windows-x64-portable.zip` anywhere and run `gitcroc.exe`. It does not install WebView2, so WebView2 must already be present (it is on current Windows 10 and 11).

The first time you run either one, a blue Microsoft Defender SmartScreen window says "Windows protected your PC":

1. Click **More info**.
2. Click **Run anyway**.

## macOS

Open `gitcroc_<version>_universal.dmg` and drag gitcroc to Applications.

The app is ad-hoc signed, which Apple silicon requires, but not notarized, so a double-click is refused with a message that Apple cannot check it for malicious software. Do one of these once:

- **Right-click, Open.** In Finder, right-click (or Control-click) gitcroc in Applications and choose **Open**, then **Open** again in the dialog. On macOS 15 and newer the dialog may only offer **Done**; then open **System Settings > Privacy & Security**, scroll down to the message about gitcroc and click **Open Anyway**.
- **Remove the quarantine attribute.** In Terminal:

  ```sh
  xattr -d com.apple.quarantine /Applications/gitcroc.app
  ```

## Linux

The Linux packages show no warning.

**AppImage.** Make the file executable, then run it:

```sh
chmod +x gitcroc_<version>_amd64.AppImage
./gitcroc_<version>_amd64.AppImage
```

AppImages need FUSE 2. If it does not start and the message mentions FUSE, install it: `sudo apt install libfuse2` (on Ubuntu 24.04 and newer, `libfuse2t64`), or `sudo dnf install fuse-libs` on Fedora.

**.deb.** Install it with apt, which also installs what it needs (WebKitGTK 4.1):

```sh
sudo apt install ./gitcroc_<version>_amd64.deb
```

**.rpm.** Install it with dnf (or zypper on openSUSE):

```sh
sudo dnf install ./gitcroc-<version>-1.x86_64.rpm
```

## Updates

gitcroc checks for a new version once a day while it runs, starting shortly after it opens. You can turn this off, or check by hand, in **Settings**, section **Updates** (see [Privacy](privacy.md#the-update-check)). Only the released packages check; a build you made yourself never does.

When a new version exists, the status bar at the bottom of the window shows **gitcroc &lt;version&gt; is available**. Click it to open **Update gitcroc**, which shows the versions, the publication date and the release notes. What happens next depends on your package:

- **Windows installer, macOS app and AppImage** update themselves. Click **Install and restart**: gitcroc downloads the new version, checks its signature, installs it and restarts. The download shows in the status bar like any other job. If a conflict editor holds edits you have not marked resolved, gitcroc first lists those files under **Update and restart?**, because restarting loses them. **Later** puts the notice away until the next version or the next start.
- **.deb, .rpm and the portable zip** do not update themselves. The dialog says so and offers **Open download page**, which opens the releases page in your browser. Install the new package as you installed the first one: `apt` and `dnf` treat it as an upgrade, and for the portable zip you replace the unzipped folder.

Every update package is signed with the project's own key, and gitcroc refuses one whose signature does not match, even though the builds are not code-signed.
