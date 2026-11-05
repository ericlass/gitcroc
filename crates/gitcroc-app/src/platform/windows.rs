//! Windows: the askpass socket is a named pipe of a random name, local to
//! the machine (D10); the file manager, the browser, the terminal and the
//! package.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::time::Duration;

use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

use std::ffi::OsString;

use gitcroc_core::Package;

use crate::update::install::{Install, nsis_parameters};

use super::{FileManager, Installation, Launch, LocalListener, LocalSockets, Terminal, WebBrowser};

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

/// Explorer opens a URL in the default browser too.
impl WebBrowser for Explorer {
    fn open(url: &str) -> Launch {
        Launch::new("explorer", [url.into()], std::env::temp_dir())
    }
}

/// The installer or the portable zip (see [`super::package`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsInstallation;

impl Installation for WindowsInstallation {
    fn package() -> Package {
        let uninstaller = std::env::current_exe()
            .ok()
            .and_then(|exe| Some(exe.parent()?.join(super::package::WINDOWS_UNINSTALLER)))
            .is_some_and(|path| path.is_file());
        super::package::windows_package(super::package::bundle_kind(), uninstaller)
    }

    fn install_update(install: &Install, package: &[u8]) -> Result<(), String> {
        match install {
            Install::WindowsInstaller => {
                let args: Vec<OsString> = std::env::args_os().skip(1).collect();
                start_installer(package, &args)
            }
            _ => Err(super::not_installed_here(install)),
        }
    }
}

/// Writes the NSIS installer to a temporary file it keeps, and starts it
/// with [`nsis_parameters`], detached, through the launcher's `Command`, so
/// no `unsafe` is needed (QA-4). The caller then exits; the installer
/// starts the new version.
fn start_installer(installer: &[u8], app_args: &[OsString]) -> Result<(), String> {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    // From `WinBase.h`: the installer gets no console and its own process
    // group, so it outlives the app.
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

    let mut file = tempfile::Builder::new()
        .prefix("gitcroc-")
        .suffix("-setup.exe")
        .tempfile()
        .map_err(|error| format!("the installer could not be written: {error}"))?;
    file.write_all(installer)
        .and_then(|()| file.as_file().sync_all())
        .map_err(|error| format!("the installer could not be written: {error}"))?;
    let (_, path) = file
        .keep()
        .map_err(|error| format!("the installer could not be kept: {error}"))?;
    super::launch::start_installer(&path, |command| {
        command
            .raw_arg(nsis_parameters(app_args))
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    })
    .map_err(|error| format!("the installer did not start: {error}"))
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

impl super::ProcessMemory for super::PlatformMemory {
    fn memory_mb() -> Result<f64, String> {
        Err("idle memory benchmarks run on Linux only".to_owned())
    }
}
