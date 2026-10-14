//! Platform code (PLAT-1): one trait per concern, implemented once per
//! platform in this module's files. A platform `cfg` appears nowhere else
//! in the crate, and here only on the modules and the `use` that pick an
//! implementation, which `plat_1_platform_code_sits_behind_traits` checks.

use std::io::{self, Read, Write};

use tokio::io::{AsyncRead, AsyncWrite};

use crate::ipc::DesktopFonts;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(not(target_os = "linux"))]
mod other;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::GtkFonts as PlatformFonts;
#[cfg(not(target_os = "linux"))]
pub use other::NoDesktopFonts as PlatformFonts;
#[cfg(unix)]
pub use unix::UnixSockets as PlatformSockets;
#[cfg(windows)]
pub use windows::NamedPipes as PlatformSockets;

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
