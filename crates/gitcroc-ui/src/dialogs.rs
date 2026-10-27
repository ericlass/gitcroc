//! The native file dialogs (SEC-2) on GPUI. The app's calls ask for them
//! from its runtime's threads; GPUI opens them on the main thread, so each
//! request goes to a task there and the answer comes back.

use std::path::PathBuf;

use gitcroc_app::platform::{DialogFuture, Dialogs, FileFilter};
use gpui_kit::{App, PathPromptOptions};
use tokio::sync::{mpsc, oneshot};

enum Request {
    Folder {
        title: String,
    },
    /// GPUI's dialogs take no filter, so any file can be picked; the app's
    /// call checks what it reads.
    File {
        title: String,
    },
    Save {
        file_name: String,
    },
}

/// The [`Dialogs`] the app opens. The requests are answered once
/// [`DialogRequests::serve`] runs.
pub struct GpuiDialogs {
    requests: mpsc::UnboundedSender<(Request, oneshot::Sender<Option<PathBuf>>)>,
}

/// The requests the app's calls send.
pub struct DialogRequests(mpsc::UnboundedReceiver<(Request, oneshot::Sender<Option<PathBuf>>)>);

/// A [`GpuiDialogs`] and its requests.
pub fn dialogs() -> (GpuiDialogs, DialogRequests) {
    let (requests, received) = mpsc::unbounded_channel();
    (GpuiDialogs { requests }, DialogRequests(received))
}

impl GpuiDialogs {
    fn ask(&self, request: Request) -> DialogFuture {
        let (reply, answer) = oneshot::channel();
        // Without the task that serves them, the answer is a cancel.
        let _ = self.requests.send((request, reply));
        Box::pin(async move { answer.await.ok().flatten() })
    }
}

impl Dialogs for GpuiDialogs {
    fn pick_folder(&self, title: &str) -> DialogFuture {
        self.ask(Request::Folder {
            title: title.to_owned(),
        })
    }

    fn pick_file(&self, title: &str, _: Option<&FileFilter>) -> DialogFuture {
        self.ask(Request::File {
            title: title.to_owned(),
        })
    }

    fn save_file(&self, _: &str, file_name: &str, _: Option<&FileFilter>) -> DialogFuture {
        self.ask(Request::Save {
            file_name: file_name.to_owned(),
        })
    }
}

impl DialogRequests {
    /// Answers the requests, one dialog at a time, until the app ends.
    pub fn serve(mut self, cx: &mut App) {
        cx.spawn(async move |cx| {
            while let Some((request, reply)) = self.0.recv().await {
                let picked = match request {
                    Request::Folder { title } => {
                        let paths = cx.update(|cx| cx.prompt_for_paths(open(title, true)));
                        paths
                            .await
                            .ok()
                            .and_then(Result::ok)
                            .flatten()
                            .and_then(first)
                    }
                    Request::File { title } => {
                        let paths = cx.update(|cx| cx.prompt_for_paths(open(title, false)));
                        paths
                            .await
                            .ok()
                            .and_then(Result::ok)
                            .flatten()
                            .and_then(first)
                    }
                    Request::Save { file_name } => {
                        let home = directories::UserDirs::new()
                            .map(|dirs| dirs.home_dir().to_owned())
                            .unwrap_or_default();
                        let path = cx.update(|cx| cx.prompt_for_new_path(&home, Some(&file_name)));
                        path.await.ok().and_then(Result::ok).flatten()
                    }
                };
                let _ = reply.send(picked);
            }
        })
        .detach();
    }
}

fn open(title: String, directories: bool) -> PathPromptOptions {
    PathPromptOptions {
        files: !directories,
        directories,
        multiple: false,
        prompt: Some(title.into()),
    }
}

fn first(paths: Vec<PathBuf>) -> Option<PathBuf> {
    paths.into_iter().next()
}
