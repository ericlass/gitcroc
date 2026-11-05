//! macOS: the file manager, the browser and the terminal, all through
//! `open`, and the package.

use std::path::Path;

use gitcroc_core::Package;

use crate::update::install::{self, Install};

use super::{FileManager, Installation, Launch, Terminal, WebBrowser};

/// `open`, which shows a folder in Finder.
#[derive(Debug, Clone, Copy, Default)]
pub struct Finder;

impl FileManager for Finder {
    fn show(path: &Path) -> Launch {
        Launch::new("open", [path.into()], path.to_owned())
    }
}

/// Terminal.app, opened in the folder.
#[derive(Debug, Clone, Copy, Default)]
pub struct TerminalApp;

impl Terminal for TerminalApp {
    fn open_in(path: &Path) -> Vec<Launch> {
        let args = ["-a".into(), "Terminal".into(), path.into()];
        vec![Launch::new("open", args, path.to_owned())]
    }
}

/// `open`, which opens a URL in the default browser.
#[derive(Debug, Clone, Copy, Default)]
pub struct Open;

impl WebBrowser for Open {
    fn open(url: &str) -> Launch {
        Launch::new("open", [url.into()], std::env::temp_dir())
    }
}

/// The `.app` (see [`super::package`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct MacInstallation;

impl Installation for MacInstallation {
    fn package() -> Package {
        super::package::macos_package(super::package::bundle_kind())
    }

    fn install_update(install: &Install, package: &[u8]) -> Result<(), String> {
        match install {
            Install::MacApp { app } => install_app(package, app),
            _ => Err(super::not_installed_here(install)),
        }
    }
}

/// Installs the `.app.tar.gz` `archive` in the place of the running `.app`
/// `app`: unpacked into a staging folder next to it, on its volume, then
/// swapped in with [`install::swap_app`]. The staging folder and the old
/// app's backup in it are removed afterwards.
fn install_app(archive: &[u8], app: &Path) -> Result<(), String> {
    let dir = app
        .parent()
        .ok_or_else(|| format!("{} has no folder", app.display()))?;
    let staging = tempfile::Builder::new()
        .prefix(".gitcroc-update-")
        .tempdir_in(dir)
        .map_err(|error| {
            install::not_writable(dir, &error)
                .unwrap_or_else(|| format!("the update could not be unpacked: {error}"))
        })?;
    let unpacked = staging.path().join("new");
    tar::Archive::new(flate2::read::GzDecoder::new(archive))
        .unpack(&unpacked)
        .map_err(|error| format!("the update could not be unpacked: {error}"))?;
    let new = install::unpacked_app(&unpacked)?;
    install::swap_app(app, &new, &staging.path().join("old.app"))
}

impl super::ProcessMemory for super::PlatformMemory {
    fn memory_mb() -> Result<f64, String> {
        Err("idle memory benchmarks run on Linux only".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[test]
    fn plat_4_mac_app_archive_is_unpacked_and_swapped_in() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("gitcroc.app");
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        fs::write(app.join("Contents/MacOS/gitcroc"), "old").unwrap();

        let mut archive = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(3);
        header.set_mode(0o755);
        header.set_cksum();
        archive
            .append_data(
                &mut header,
                "gitcroc.app/Contents/MacOS/gitcroc",
                &b"new"[..],
            )
            .unwrap();
        let archive = archive.into_inner().unwrap().finish().unwrap();

        super::install_app(&archive, &app).unwrap();
        assert_eq!(
            fs::read_to_string(app.join("Contents/MacOS/gitcroc")).unwrap(),
            "new"
        );
        // The staging folder and the backup are gone.
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["gitcroc.app"]);
    }
}
