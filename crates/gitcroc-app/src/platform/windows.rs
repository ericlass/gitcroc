//! Windows: the askpass socket is a named pipe of a random name, local to
//! the machine (D10); the file manager and the terminal.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::time::Duration;

use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

use super::{FileManager, Launch, LocalListener, LocalSockets, Terminal};

/// `ERROR_PIPE_BUSY`: every instance of the pipe is taken for now.
const PIPE_BUSY: i32 = 231;

/// How often a client tries a busy pipe, and how long it waits in between.
const CONNECT_TRIES: u32 = 50;
const CONNECT_PAUSE: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, Copy, Default)]
pub struct NamedPipes;

/// The pipe instance waiting for the next client.
#[derive(Debug)]
pub struct PipeListener {
    next: NamedPipeServer,
    address: String,
}

impl LocalSockets for NamedPipes {
    type Listener = PipeListener;
    type Client = File;

    fn listen() -> io::Result<PipeListener> {
        let address = format!(
            r"\\.\pipe\gitcroc-askpass-{}",
            uuid::Uuid::new_v4().simple()
        );
        let next = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(&address)?;
        Ok(PipeListener { next, address })
    }

    fn connect(address: &str) -> io::Result<File> {
        let mut tries = 0;
        loop {
            match OpenOptions::new().read(true).write(true).open(address) {
                Err(error) if error.raw_os_error() == Some(PIPE_BUSY) && tries < CONNECT_TRIES => {
                    tries += 1;
                    std::thread::sleep(CONNECT_PAUSE);
                }
                result => return result,
            }
        }
    }
}

impl LocalListener for PipeListener {
    type Stream = NamedPipeServer;

    fn address(&self) -> &str {
        &self.address
    }

    async fn accept(&mut self) -> io::Result<NamedPipeServer> {
        self.next.connect().await?;
        let next = ServerOptions::new()
            .reject_remote_clients(true)
            .create(&self.address)?;
        Ok(std::mem::replace(&mut self.next, next))
    }
}

/// Explorer, which shows a folder.
#[derive(Debug, Clone, Copy, Default)]
pub struct Explorer;

impl FileManager for Explorer {
    fn show(path: &Path) -> Launch {
        Launch::new("explorer", [path.into()], path.to_owned())
    }
}

/// Windows Terminal when it is installed, else `cmd` in a console of its
/// own, which Windows gives a console program the app starts.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsTerminal;

impl Terminal for WindowsTerminal {
    fn open_in(path: &Path) -> Vec<Launch> {
        vec![
            Launch::new("wt", ["-d".into(), path.into()], path.to_owned()),
            Launch::new("cmd", [], path.to_owned()),
        ]
    }
}
