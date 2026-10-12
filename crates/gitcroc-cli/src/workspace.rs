//! `gitcroc-cli workspace`: show the workspace, add repos to it and remove
//! them (GRP-1, GRP-3), as JSON. Progress goes to stderr.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use gitcroc_core::{
    ConfigDir, GitRunner, GitSetup, JobIds, PlatformConfigDir, RepoId, Workspaces, add_path,
};

pub const USAGE: &str = "workspace [--config-dir <dir>] show|add <path>|remove <repo-id>";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Show,
    Add(PathBuf),
    Remove(RepoId),
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
    let action = match rest {
        [action] if action == "show" => Action::Show,
        [action, path] if action == "add" => Action::Add(PathBuf::from(path)),
        [action, id] if action == "remove" => Action::Remove(RepoId(id.to_str()?.to_owned())),
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

/// Prints the workspace, or the add report, as JSON. Exits 0 on success and
/// 2 on any error, such as a read-only workspace or no usable git.
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
    let views = workspaces.views();
    let workspace = views
        .first()
        .ok_or_else(|| "no workspace is open".to_owned())?;
    if let Some(warning) = &workspace.warning {
        eprintln!("warning: the workspace is read-only: {warning:?}");
    }
    let value = match options.action {
        Action::Show => serde_json::to_value(workspace),
        Action::Remove(repo) => {
            let view = workspaces
                .remove_repo(&workspace.id, &repo)
                .map_err(|error| error.to_string())?;
            serde_json::to_value(view)
        }
        Action::Add(path) => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| format!("cannot start the async runtime: {error}"))?;
            let setup = GitSetup::new();
            let status = runtime.block_on(setup.detect());
            let git = runtime
                .block_on(setup.git())
                .ok_or_else(|| format!("no usable git: {status:?}"))?;
            let job = JobIds::default().next();
            let report = runtime
                .block_on(add_path(
                    workspaces.clone(),
                    workspace.id.clone(),
                    path,
                    GitRunner::new(git.path),
                    job,
                    |progress| eprintln!("{} {}/{}", progress.label, progress.done, progress.total),
                ))
                .map_err(|error| error.to_string())?;
            serde_json::to_value(report)
        }
    };
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
        assert_eq!(parse_args(&args(&[])), None);
        assert_eq!(parse_args(&args(&["add"])), None);
        assert_eq!(parse_args(&args(&["--config-dir", "c"])), None);
        assert_eq!(parse_args(&args(&["show", "extra"])), None);
    }
}
