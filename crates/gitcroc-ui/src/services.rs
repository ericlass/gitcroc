//! The UI's way to the app's services (ARCH-7): every call starts on the
//! tokio runtime and is awaited in a GPUI task, so no core work runs on the
//! UI thread.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use gitcroc_app::app::App;
use gitcroc_app::platform::{FileManager, Launch, PlatformFileManager, PlatformTerminal, Terminal};
use std::path::Path;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

/// The app, and the runtime its calls run on. Cheap to clone.
#[derive(Clone)]
pub struct Services {
    app: App,
    runtime: Handle,
    pub(crate) file_manager: fn(&Path) -> Launch,
    pub(crate) terminal: fn(&Path) -> Vec<Launch>,
}

impl Services {
    pub fn new(app: App, runtime: Handle) -> Self {
        Self {
            app,
            runtime,
            file_manager: PlatformFileManager::show,
            terminal: PlatformTerminal::open_in,
        }
    }

    /// Chooses platform implementations without changing the core's repo paths.
    pub fn with_repo_openers<F: FileManager, T: Terminal>(mut self) -> Self {
        self.file_manager = F::show;
        self.terminal = T::open_in;
        self
    }

    pub fn app(&self) -> &App {
        &self.app
    }

    /// Starts `call` on the runtime; await the [`Call`] in a GPUI task.
    pub fn call<F>(&self, call: F) -> Call<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        Call(self.runtime.spawn(call))
    }

    /// Starts `call` with the app on the runtime; see [`Self::call`].
    pub fn with_app<F>(&self, call: impl FnOnce(App) -> F) -> Call<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.call(call(self.app.clone()))
    }
}

/// A call running on the runtime. It resolves to the call's result, or
/// `None` when the call panicked or the runtime shut down.
pub struct Call<T>(JoinHandle<T>);

impl<T> Future for Call<T> {
    type Output = Option<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx).map(Result::ok)
    }
}
