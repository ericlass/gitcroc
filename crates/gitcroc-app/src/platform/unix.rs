//! Linux and macOS: the askpass socket is a Unix domain socket in a new
//! directory only the user can open (D10).

use std::io;
use std::os::unix::net::UnixStream;

use tempfile::TempDir;
use tokio::net::UnixListener;

use super::{LocalListener, LocalSockets};

#[derive(Debug, Clone, Copy, Default)]
pub struct UnixSockets;

/// The listening socket; its directory is removed with it.
#[derive(Debug)]
pub struct UnixSocketListener {
    listener: UnixListener,
    address: String,
    _dir: TempDir,
}

impl LocalSockets for UnixSockets {
    type Listener = UnixSocketListener;
    type Client = UnixStream;

    fn listen() -> io::Result<UnixSocketListener> {
        // `tempfile` creates the directory with mode 0700. The name is
        // short: a socket path has a limit of about 100 bytes.
        let dir = tempfile::Builder::new().prefix("gitcroc-").tempdir()?;
        let path = dir.path().join("askpass");
        let listener = UnixListener::bind(&path)?;
        let address = path
            .to_str()
            .ok_or_else(|| io::Error::other("the temporary directory is not valid UTF-8"))?
            .to_owned();
        Ok(UnixSocketListener {
            listener,
            address,
            _dir: dir,
        })
    }

    fn connect(address: &str) -> io::Result<UnixStream> {
        UnixStream::connect(address)
    }
}

impl LocalListener for UnixSocketListener {
    type Stream = tokio::net::UnixStream;

    fn address(&self) -> &str {
        &self.address
    }

    async fn accept(&mut self) -> io::Result<tokio::net::UnixStream> {
        let (stream, _) = self.listener.accept().await?;
        Ok(stream)
    }
}
