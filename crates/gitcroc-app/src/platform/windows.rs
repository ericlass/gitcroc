//! Windows: the askpass socket is a named pipe of a random name, local to
//! the machine (D10).

use std::fs::{File, OpenOptions};
use std::io;
use std::time::Duration;

use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

use super::{LocalListener, LocalSockets};

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
