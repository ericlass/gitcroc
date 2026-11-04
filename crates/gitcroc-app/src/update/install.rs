//! Installing a verified update package, per package (PLAT-4), as Tauri's
//! updater 2.14 did for 1.0.0-rc.1. Each step takes paths and bytes, so
//! every platform's steps are tested on every platform; only starting the
//! Windows installer and unpacking the macOS archive need their own OS, and
//! sit behind `Installation::install_update` (PLAT-1).
//!
//! - **Windows (NSIS installer):** written to a temporary file and started
//!   with the arguments Tauri passes in passive mode with restart; the app
//!   then exits, and the installer starts the new version.
//! - **macOS (`.app.tar.gz`):** unpacked next to the running `.app`, which
//!   is swapped for the new one with renames; the old one comes back if the
//!   swap fails.
//! - **AppImage:** the new file replaces `$APPIMAGE` with a rename and keeps
//!   its mode.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use gitcroc_core::Package;

use super::RELEASES_PAGE;

/// Where a verified update goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Install {
    /// The NSIS installer's app.
    WindowsInstaller,
    /// The running `.app` bundle.
    MacApp { app: PathBuf },
    /// The running AppImage, `$APPIMAGE`.
    AppImage { file: PathBuf },
}

impl Install {
    /// Where the running app's update goes; an error for the packages that
    /// only notify.
    pub fn running(package: Package) -> Result<Self, String> {
        match package {
            Package::WindowsInstaller => Ok(Self::WindowsInstaller),
            Package::MacApp => {
                let exe = std::env::current_exe()
                    .map_err(|error| format!("the running app was not found: {error}"))?;
                running_app(&exe)
                    .map(|app| Self::MacApp { app })
                    .ok_or_else(|| format!("{} is not inside an .app", exe.display()))
            }
            Package::AppImage => std::env::var_os("APPIMAGE")
                .filter(|file| !file.is_empty())
                .map(|file| Self::AppImage {
                    file: PathBuf::from(file),
                })
                .ok_or_else(|| "APPIMAGE is not set".to_owned()),
            Package::WindowsPortable | Package::Deb | Package::Rpm | Package::Unknown => {
                Err("this package does not install updates".to_owned())
            }
        }
    }
}

/// The `.app` bundle the executable `exe` is in.
pub fn running_app(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|dir| dir.extension() == Some(OsStr::new("app")))
        .map(Path::to_path_buf)
}

/// The NSIS installer's parameters, as `tauri-plugin-updater` 2.14.0
/// builds them in passive mode with restart: `/P /UPDATE /R /ARGS`, then
/// the app's own arguments, `app_args`, escaped so NSIS does not take one
/// for its own. Passed to the installer as written.
pub fn nsis_parameters(app_args: &[OsString]) -> OsString {
    let mut parameters = OsString::from("/P /UPDATE /R /ARGS");
    for arg in app_args {
        parameters.push(" ");
        parameters.push(nsis_arg(arg));
    }
    parameters
}

/// One argument quoted as Tauri's `escape_nsis_current_exe_arg` quotes it:
/// in quotes when it is empty or has a space, a tab or a `/`, and with the
/// backslashes before a quote doubled. Text that is not Unicode, which
/// Windows allows, is passed with its unpaired surrogates replaced.
fn nsis_arg(arg: &OsStr) -> String {
    let arg = arg.to_string_lossy();
    let quote = arg.is_empty() || arg.contains([' ', '\t', '/']);
    let mut escaped = String::with_capacity(arg.len() + 2);
    if quote {
        escaped.push('"');
    }
    let mut backslashes = 0;
    for c in arg.chars() {
        if c == '\\' {
            backslashes += 1;
        } else {
            if c == '"' {
                escaped.extend(std::iter::repeat_n('\\', backslashes + 1));
            }
            backslashes = 0;
        }
        escaped.push(c);
    }
    if quote {
        escaped.extend(std::iter::repeat_n('\\', backslashes));
        escaped.push('"');
    }
    escaped
}

/// Replaces the AppImage `file` with `appimage`, which must be ELF: written
/// to a temporary file next to it, given its mode, then renamed over it. A
/// failure leaves `file` as it was.
pub fn replace_appimage(appimage: &[u8], file: &Path) -> Result<(), String> {
    if !appimage.starts_with(b"\x7fELF") {
        return Err("the update is not an AppImage".to_owned());
    }
    let failed = |error: io::Error| format!("{} could not be replaced: {error}", file.display());
    let dir = file
        .parent()
        .ok_or_else(|| failed(io::ErrorKind::NotFound.into()))?;
    let permissions = fs::metadata(file).map_err(failed)?.permissions();
    let mut temp = tempfile::Builder::new()
        .prefix(".gitcroc-update-")
        .tempfile_in(dir)
        .map_err(|error| not_writable(dir, &error).unwrap_or_else(|| failed(error)))?;
    temp.write_all(appimage)
        .and_then(|()| temp.as_file().sync_all())
        .and_then(|()| fs::set_permissions(temp.path(), permissions))
        .map_err(failed)?;
    temp.persist(file)
        .map(drop)
        .map_err(|error| failed(error.error))
}

/// Puts the `.app` at `new` in the place of `app`, moving `app` to
/// `backup` first. `backup` and `new` must be on `app`'s volume, so each
/// step is a rename. If `new` cannot be moved, `app` comes back.
pub fn swap_app(app: &Path, new: &Path, backup: &Path) -> Result<(), String> {
    fs::rename(app, backup)
        .map_err(|error| format!("{} could not be moved: {error}", app.display()))?;
    if let Err(error) = fs::rename(new, app) {
        let restored = fs::rename(backup, app);
        return Err(match restored {
            Ok(()) => format!("the new version could not be moved into place: {error}"),
            Err(restore) => format!(
                "the new version could not be moved into place ({error}), and the old one \
                 could not be put back from {}: {restore}",
                backup.display()
            ),
        });
    }
    Ok(())
}

/// The single `.app` an archive unpacked into `dir`.
pub fn unpacked_app(dir: &Path) -> Result<PathBuf, String> {
    let unreadable = |error: io::Error| format!("the unpacked update could not be read: {error}");
    let mut apps = Vec::new();
    for entry in fs::read_dir(dir).map_err(unreadable)? {
        let path = entry.map_err(unreadable)?.path();
        if path.extension() == Some(OsStr::new("app")) && path.is_dir() {
            apps.push(path);
        }
    }
    match <[PathBuf; 1]>::try_from(apps) {
        Ok([app]) => Ok(app),
        Err(apps) => Err(format!("the update holds {} apps, not one", apps.len())),
    }
}

/// The message for a folder the user may not write to: the update is not
/// installed there with an administrator's password; the download page
/// has the new version.
pub fn not_writable(dir: &Path, error: &io::Error) -> Option<String> {
    (error.kind() == io::ErrorKind::PermissionDenied).then(|| {
        format!(
            "gitcroc cannot write to {}; download the new version from {RELEASES_PAGE}",
            dir.display()
        )
    })
}
