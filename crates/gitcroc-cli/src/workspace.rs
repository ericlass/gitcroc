//! `gitcroc-cli workspace`: show the active workspace, add repos to it and
//! remove them (GRP-1, GRP-3); list, create, open, close, rename and delete
//! workspaces (UI-27, UI-33); export one to a file and import one from a
//! file (DATA-2). Prints JSON. Progress goes to stderr.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use gitcroc_core::{
    ConfigDir, GitRunner, GitSetup, JobIds, PlatformConfigDir, RepoId, WorkspaceId, Workspaces,
    add_path,
};

pub const USAGE: &str = "workspace [--config-dir <dir>] show|add <path>|remove <repo-id>|list|\
create <name>|open <id>|close <id>|rename <id> <name>|delete <id>|export <id> <file>|import <file>";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Show,
    Add(PathBuf),
    Remove(RepoId),
    List,
    Create(String),
    Open(WorkspaceId),
    Close(WorkspaceId),
    Rename(WorkspaceId, String),
    /// The command line is the confirmation.
    Delete(WorkspaceId),
    Export(WorkspaceId, PathBuf),
    Import(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The platform config dir when `None`.
    pub config_dir: Option<PathBuf>,
    pub action: Action,
}

/// Parses the arguments after `workspace`.
pub fn parse_args(args: &[OsString]) -> Option<Options> {
    let (config_dir, rest) = match args {
        [flag, dir, rest @ ..] if flag == "--config-dir" => (Some(PathBuf::from(dir)), rest),
        rest => (None, rest),
    };
    let text = |arg: &OsString| arg.to_str().map(str::to_owned);
    let workspace = |arg: &OsString| text(arg).map(WorkspaceId);
    let action = match rest {
        [action] if action == "show" => Action::Show,
        [action, path] if action == "add" => Action::Add(PathBuf::from(path)),
        [action, id] if action == "remove" => Action::Remove(RepoId(text(id)?)),
        [action] if action == "list" => Action::List,
        [action, name] if action == "create" => Action::Create(text(name)?),
        [action, id] if action == "open" => Action::Open(workspace(id)?),
        [action, id] if action == "close" => Action::Close(workspace(id)?),
        [action, id, name] if action == "rename" => Action::Rename(workspace(id)?, text(name)?),
        [action, id] if action == "delete" => Action::Delete(workspace(id)?),
        [action, id, file] if action == "export" => {
            Action::Export(workspace(id)?, PathBuf::from(file))
        }
        [action, file] if action == "import" => Action::Import(PathBuf::from(file)),
        _ => return None,
    };
    Some(Options { config_dir, action })
}

/// The config dir given on the command line.
pub(crate) struct GivenConfigDir(pub PathBuf);

impl ConfigDir for GivenConfigDir {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.0.clone())
    }
}

/// Prints the workspace, the add report, the list of workspaces, or the
/// open tabs after a change, as JSON. Exits 0 on success and 2 on any
/// error, such as a read-only workspace, a refused name or no usable git.
pub fn run(options: Options) -> ExitCode {
    match act(options) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn act(options: Options) -> Result<String, String> {
    let workspaces = Arc::new(match &options.config_dir {
        Some(dir) => Workspaces::load(&GivenConfigDir(dir.clone())),
        None => Workspaces::load(&PlatformConfigDir),
    });
    for missing in workspaces.tabs().missing {
        eprintln!("warning: the workspace file {missing} is gone; its tab was left out");
    }
    let tabs = |result: Result<_, gitcroc_core::WorkspaceError>| {
        result
            .map(|_| serde_json::to_value(workspaces.tabs()))
            .map_err(|error| error.to_string())
    };
    let value = match options.action {
        Action::List => serde_json::to_value(workspaces.list().map_err(|error| error.to_string())?),
        Action::Create(name) => tabs(workspaces.create(&name).map(drop))?,
        Action::Open(id) => tabs(workspaces.open(&id).map(drop))?,
        Action::Close(id) => tabs(workspaces.close(&id))?,
        Action::Rename(id, name) => tabs(workspaces.rename(&id, &name))?,
        Action::Export(id, file) => serde_json::to_value(
            workspaces
                .export(&id, &file)
                .map_err(|error| error.to_string())?,
        ),
        Action::Import(file) => {
            let (_, git) = runtime_and_git()?;
            serde_json::to_value(
                workspaces
                    .import(&file, &git)
                    .map_err(|error| error.to_string())?,
            )
        }
        Action::Delete(id) => {
            workspaces
                .delete(&id, true)
                .map_err(|error| error.to_string())?;
            serde_json::to_value(workspaces.list().map_err(|error| error.to_string())?)
        }
        action => return on_active(&workspaces, action),
    };
    to_json(value)
}

/// Shows the active workspace, or adds or removes a repo of it.
fn on_active(workspaces: &Arc<Workspaces>, action: Action) -> Result<String, String> {
    let workspace = workspaces
        .active()
        .and_then(|id| workspaces.view(&id).ok())
        .ok_or_else(|| "no workspace is open".to_owned())?;
    if let Some(warning) = &workspace.warning {
        eprintln!("warning: the workspace is read-only: {warning:?}");
    }
    let value = match action {
        Action::Show => serde_json::to_value(&workspace),
        Action::Remove(repo) => {
            let view = workspaces
                .remove_repo(&workspace.id, &repo)
                .map_err(|error| error.to_string())?;
            serde_json::to_value(view)
        }
        Action::Add(path) => {
            let (runtime, git) = runtime_and_git()?;
            let job = JobIds::default().next();
            let report = runtime
                .block_on(add_path(
                    workspaces.clone(),
                    workspace.id.clone(),
                    path,
                    git,
                    job,
                    |progress| eprintln!("{} {}/{}", progress.label, progress.done, progress.total),
                ))
                .map_err(|error| error.to_string())?;
            serde_json::to_value(report)
        }
        _ => return Err("not an action on the active workspace".to_owned()),
    };
    to_json(value)
}

/// An async runtime and a runner for the detected git.
fn runtime_and_git() -> Result<(tokio::runtime::Runtime, GitRunner), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("cannot start the async runtime: {error}"))?;
    let setup = GitSetup::new();
    let status = runtime.block_on(setup.detect());
    let git = runtime
        .block_on(setup.git())
        .ok_or_else(|| format!("no usable git: {status:?}"))?;
    Ok((runtime, GitRunner::new(git.path)))
}

fn to_json(value: serde_json::Result<serde_json::Value>) -> Result<String, String> {
    value
        .and_then(|value| serde_json::to_string_pretty(&value))
        .map_err(|error| format!("cannot write the result as JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arch_5_workspace_arguments() {
        let args = |list: &[&str]| -> Vec<OsString> { list.iter().map(OsString::from).collect() };
        assert_eq!(
            parse_args(&args(&["show"])),
            Some(Options {
                config_dir: None,
                action: Action::Show
            })
        );
        assert_eq!(
            parse_args(&args(&["--config-dir", "c", "add", "repos"])),
            Some(Options {
                config_dir: Some(PathBuf::from("c")),
                action: Action::Add(PathBuf::from("repos"))
            })
        );
        assert_eq!(
            parse_args(&args(&["remove", "r2"])),
            Some(Options {
                config_dir: None,
                action: Action::Remove(RepoId("r2".to_owned()))
            })
        );
        assert_eq!(
            parse_args(&args(&["create", "C#"])).map(|options| options.action),
            Some(Action::Create("C#".to_owned()))
        );
        assert_eq!(
            parse_args(&args(&["rename", "w1", "Java"])).map(|options| options.action),
            Some(Action::Rename(
                WorkspaceId("w1".to_owned()),
                "Java".to_owned()
            ))
        );
        assert_eq!(
            parse_args(&args(&["delete", "w1"])).map(|options| options.action),
            Some(Action::Delete(WorkspaceId("w1".to_owned())))
        );
        assert_eq!(
            parse_args(&args(&["export", "w1", "team.toml"])).map(|options| options.action),
            Some(Action::Export(
                WorkspaceId("w1".to_owned()),
                PathBuf::from("team.toml")
            ))
        );
        assert_eq!(
            parse_args(&args(&["import", "team.toml"])).map(|options| options.action),
            Some(Action::Import(PathBuf::from("team.toml")))
        );
        assert_eq!(parse_args(&args(&["export", "w1"])), None);
        assert_eq!(parse_args(&args(&["create"])), None);
        assert_eq!(parse_args(&args(&[])), None);
        assert_eq!(parse_args(&args(&["add"])), None);
        assert_eq!(parse_args(&args(&["--config-dir", "c"])), None);
        assert_eq!(parse_args(&args(&["show", "extra"])), None);
    }
}
