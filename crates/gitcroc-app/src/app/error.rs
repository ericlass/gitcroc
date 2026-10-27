//! The error every `App` method returns: what the Tauri commands returned as
//! `IpcError`, with the same codes and messages.

use gitcroc_core::{
    BranchError, ChangesError, CloneError, CommitGraphError, FsmonitorError, GroupError,
    HistoryViewError, IntegrateError, JobError, RemoteError, ResetError, SettingsError, StashError,
    StatusError, StatusReadError, SyncError, TagError, TreeError, UpdateError, WorkspaceError,
};

use crate::ai::AiError;
use crate::platform::LaunchError;

/// The error every [`crate::app::App`] method returns.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: AppErrorCode,
    pub message: String,
    /// What git printed on stderr, when the error came from a git command.
    pub git_stderr: Option<String>,
}

/// What kind of failure an [`AppError`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppErrorCode {
    /// A bug or an environment problem the user cannot fix from the UI.
    Internal,
    /// The settings are read-only (DATA-1); `settings_get` says why.
    SettingsReadOnly,
    /// The workspace is read-only (DATA-1); its view says why.
    WorkspaceReadOnly,
    /// No open workspace has the id, or the workspace has no such repo.
    NotFound,
    /// No usable git was found (PLAT-3); `git_status` says why.
    GitUnavailable,
    /// The request was refused as it stands, such as a workspace name that
    /// is taken; the message says why and the user can change it.
    Refused,
    /// AI features are off (AI-16); nothing was run.
    AiOff,
}

impl AppError {
    /// An [`AppErrorCode::Internal`] error.
    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::new(AppErrorCode::Internal, message)
    }

    /// An [`AppErrorCode::Refused`] error.
    pub(crate) fn refused(message: impl Into<String>) -> Self {
        Self::new(AppErrorCode::Refused, message)
    }

    fn new(code: AppErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            git_stderr: None,
        }
    }
}

/// Work moved off the caller's thread panicked or was cancelled.
impl From<tokio::task::JoinError> for AppError {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::internal(error.to_string())
    }
}

impl From<SettingsError> for AppError {
    fn from(error: SettingsError) -> Self {
        let code = match error {
            SettingsError::ReadOnly => AppErrorCode::SettingsReadOnly,
            SettingsError::Parallelism | SettingsError::Ai(_) | SettingsError::GitPathNotText => {
                AppErrorCode::Refused
            }
            SettingsError::Write { .. } | SettingsError::Encode(_) => AppErrorCode::Internal,
        };
        Self::new(code, error.to_string())
    }
}

impl From<AiError> for AppError {
    fn from(error: AiError) -> Self {
        let code = match error {
            AiError::Settings(error) => return error.into(),
            AiError::Off => AppErrorCode::AiOff,
            AiError::Invalid(_)
            | AiError::Keychain(_)
            | AiError::NeedsKey
            | AiError::TakesNoKey(_)
            | AiError::NotOffered(_)
            | AiError::ConsentChanged { .. } => AppErrorCode::Refused,
        };
        Self::new(code, error.to_string())
    }
}

impl From<WorkspaceError> for AppError {
    fn from(error: WorkspaceError) -> Self {
        let code = match error {
            WorkspaceError::ReadOnly => AppErrorCode::WorkspaceReadOnly,
            WorkspaceError::UnknownWorkspace(_)
            | WorkspaceError::UnknownRepo(_)
            | WorkspaceError::UnknownGroup(_)
            | WorkspaceError::NoSuchWorkspace(_) => AppErrorCode::NotFound,
            WorkspaceError::BlankName
            | WorkspaceError::NameTooLong
            | WorkspaceError::NameTaken(_)
            | WorkspaceError::IsOpen
            | WorkspaceError::NotConfirmed
            | WorkspaceError::BadOrder
            | WorkspaceError::BlankGroupName
            | WorkspaceError::GroupNameTooLong
            | WorkspaceError::GroupNameTaken(_)
            | WorkspaceError::GroupNameReserved
            | WorkspaceError::BadGroupOrder
            | WorkspaceError::NotImported { .. }
            | WorkspaceError::AlreadyPresent(_) => AppErrorCode::Refused,
            WorkspaceError::Write { .. }
            | WorkspaceError::Read { .. }
            | WorkspaceError::Delete { .. }
            | WorkspaceError::Encode(_)
            | WorkspaceError::Stopped(_) => AppErrorCode::Internal,
        };
        Self::new(code, error.to_string())
    }
}

impl From<TreeError> for AppError {
    fn from(error: TreeError) -> Self {
        match error {
            TreeError::Workspace(error) => error.into(),
            TreeError::UnknownGroup(group) => WorkspaceError::UnknownGroup(group).into(),
            TreeError::Open(_) | TreeError::Refs(_) | TreeError::Stopped(_) => {
                Self::internal(error.to_string())
            }
        }
    }
}

impl From<CommitGraphError> for AppError {
    fn from(error: CommitGraphError) -> Self {
        match error {
            CommitGraphError::Workspace(error)
            | CommitGraphError::Job(JobError::Workspace(error)) => error.into(),
            CommitGraphError::Job(JobError::NoRepo) => Self::internal(error.to_string()),
        }
    }
}

impl From<HistoryViewError> for AppError {
    fn from(error: HistoryViewError) -> Self {
        match error {
            HistoryViewError::Workspace(error) => error.into(),
            HistoryViewError::Open(_)
            | HistoryViewError::Page(_)
            | HistoryViewError::NoPage
            | HistoryViewError::NoRepo => Self::internal(error.to_string()),
        }
    }
}

impl From<GroupError> for AppError {
    fn from(error: GroupError) -> Self {
        match error {
            GroupError::Workspace(error) | GroupError::Job(JobError::Workspace(error)) => {
                error.into()
            }
            GroupError::Job(JobError::NoRepo) => Self::internal(error.to_string()),
            GroupError::Name(_)
            | GroupError::Refused(_)
            | GroupError::NoRepo
            | GroupError::UnknownPreflight
            | GroupError::NotReady
            | GroupError::NeedsConfirmation
            | GroupError::WorkspaceChanged
            | GroupError::UnknownJob
            | GroupError::NotEnded
            | GroupError::NothingFailed => Self::refused(error.to_string()),
        }
    }
}

/// `From` for the error of an operation on a repo, which has the variants
/// `Workspace`, `Job` and `Status`: git's refusal to read `$what` keeps
/// git's message for the user (SEC-6).
macro_rules! from_op_error {
    ($error:ident, $what:literal) => {
        impl From<$error> for AppError {
            fn from(error: $error) -> Self {
                match error {
                    $error::Workspace(error) | $error::Job(JobError::Workspace(error)) => {
                        error.into()
                    }
                    $error::Status(StatusReadError::Read(StatusError::Refused { message })) => {
                        Self {
                            git_stderr: Some(message),
                            ..Self::internal(concat!("git refused to read ", $what))
                        }
                    }
                    other => Self::internal(other.to_string()),
                }
            }
        }
    };
}

from_op_error!(BranchError, "the repo");

from_op_error!(SyncError, "the repo");

from_op_error!(IntegrateError, "the repo");

from_op_error!(TagError, "the repo");

from_op_error!(ResetError, "the repo");

from_op_error!(RemoteError, "the repo");

from_op_error!(FsmonitorError, "the repo");

from_op_error!(StashError, "the repo");

from_op_error!(ChangesError, "the working copy");

impl From<CloneError> for AppError {
    fn from(error: CloneError) -> Self {
        match error {
            CloneError::Workspace(error) | CloneError::Job(JobError::Workspace(error)) => {
                error.into()
            }
            other => Self::internal(other.to_string()),
        }
    }
}

impl From<LaunchError> for AppError {
    fn from(error: LaunchError) -> Self {
        Self::internal(error.to_string())
    }
}

impl From<UpdateError> for AppError {
    fn from(error: UpdateError) -> Self {
        match error {
            UpdateError::Job(JobError::Workspace(error)) => error.into(),
            UpdateError::Job(JobError::NoRepo) => Self::internal(error.to_string()),
            UpdateError::NoUpdate | UpdateError::OnlyNotifies | UpdateError::JobsRunning => {
                Self::refused(error.to_string())
            }
        }
    }
}
