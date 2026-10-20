//! Platform code (PLAT-1): one trait per concern, implemented once per
//! platform in this module's files. A platform `cfg` appears nowhere else
//! in the crate, and here only on the modules and the `use` that pick an
//! implementation, which `plat_1_platform_code_sits_behind_traits` checks.

use std::io::{self, Read, Write};
use std::path::Path;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::ipc::DesktopFonts;

mod keychain;
mod launch;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "linux"))]
mod other;
pub mod package;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub use keychain::{KEYCHAIN_SERVICE, KeychainError, MemoryKeychain, OsKeychain, keychain_account};
pub use launch::{Launch, LaunchError, start};
#[cfg(target_os = "linux")]
pub use linux::GtkFonts as PlatformFonts;
#[cfg(target_os = "linux")]
pub use linux::LinuxInstallation as PlatformInstallation;
#[cfg(target_os = "linux")]
pub use linux::Terminals as PlatformTerminal;
#[cfg(target_os = "linux")]
pub use linux::XdgOpen as PlatformBrowser;
#[cfg(target_os = "linux")]
pub use linux::XdgOpen as PlatformFileManager;
#[cfg(target_os = "macos")]
pub use macos::Finder as PlatformFileManager;
#[cfg(target_os = "macos")]
pub use macos::MacInstallation as PlatformInstallation;
#[cfg(target_os = "macos")]
pub use macos::Open as PlatformBrowser;
#[cfg(target_os = "macos")]
pub use macos::TerminalApp as PlatformTerminal;
#[cfg(not(target_os = "linux"))]
pub use other::NoDesktopFonts as PlatformFonts;
#[cfg(unix)]
pub use unix::UnixSockets as PlatformSockets;
#[cfg(windows)]
pub use windows::Explorer as PlatformFileManager;
#[cfg(windows)]
pub use windows::Explorer as PlatformBrowser;
#[cfg(windows)]
pub use windows::NamedPipes as PlatformSockets;
#[cfg(windows)]
pub use windows::WindowsInstallation as PlatformInstallation;
#[cfg(windows)]
pub use windows::WindowsTerminal as PlatformTerminal;

/// The system file manager (section 6 "Repos").
pub trait FileManager {
    /// What shows folder `path` in it.
    fn show(path: &Path) -> Launch;
}

/// The default web browser, for the download page (PLAT-4).
pub trait WebBrowser {
    /// What opens `url` in it.
    fn open(url: &str) -> Launch;
}

/// The package the running app came from (PLAT-4); see [`package`].
pub trait Installation {
    fn package() -> gitcroc_core::Package;
}

/// A terminal (section 6 "Repos").
pub trait Terminal {
    /// What may open a terminal in folder `path`, in the order to try: the
    /// first one that is installed is started.
    fn open_in(path: &Path) -> Vec<Launch>;
}

/// The desktop's UI and monospace font families (UI-26).
pub trait DesktopFontSource {
    /// Read once, on the main thread, after the toolkit is up.
    fn read(&self) -> DesktopFonts;
}

/// A local socket that only processes of this machine reach: the askpass
/// helper's way to the app (D10). A Unix domain socket in a directory only
/// the user can open, or a named pipe on Windows.
pub trait LocalSockets {
    type Listener: LocalListener;
    type Client: Read + Write;

    /// A new socket at an address of its own. Needs a tokio runtime.
    fn listen() -> io::Result<Self::Listener>;

    /// Connects to the socket at `address`, blocking.
    fn connect(address: &str) -> io::Result<Self::Client>;
}

/// The app's end of a [`LocalSockets`] socket.
pub trait LocalListener: Send + 'static {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    /// Where clients connect: a socket path or a pipe name.
    fn address(&self) -> &str;

    /// Waits for the next client.
    fn accept(&mut self) -> impl Future<Output = io::Result<Self::Stream>> + Send;
}

/// The OS keychain, where AI keys live and nowhere else (SEC-1, D11). The
/// calls block: the Secret Service is reached over D-Bus.
pub trait Keychain: Send + Sync {
    /// The secret of `account`, or `None` when it has none.
    fn read(&self, account: &str) -> Result<Option<String>, KeychainError>;

    /// Stores `secret` for `account`, replacing any.
    fn write(&self, account: &str, secret: &str) -> Result<(), KeychainError>;

    /// Removes the secret of `account`; one that has none is fine.
    fn remove(&self, account: &str) -> Result<(), KeychainError>;
}
