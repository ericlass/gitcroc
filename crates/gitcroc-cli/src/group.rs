//! `gitcroc-cli group`: list, create, rename and delete the groups of the
//! active workspace, and add and remove their repos (GRP-2, GRP-3), and run
//! the test operation on a group (`group_run`). Groups are named by name,
//! ignoring case, as their ids last one run only. Also
//! `gitcroc-cli tree`: the active workspace's tree skeleton (UI-1). Both
//! print JSON.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use gitcroc_core::{
    GroupId, PlatformCacheDir, PlatformConfigDir, RepoId, Summaries, SummaryCache, Trees,
    WorkspaceId, Workspaces,
};

use crate::summaries::GivenCacheDir;
use crate::workspace::GivenConfigDir;

pub const USAGE: &str = "group [--config-dir <dir>] list|create <name> [<repo-id>...]|\
rename <group> <name>|delete <group>|add <group> <repo-id>...|remove <group> <repo-id>...|\
run <group> --test-op [--confirm]";

pub const TREE_USAGE: &str = "tree [--config-dir <dir>] [--cache-dir <dir>]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    List,
    Create(String, Vec<RepoId>),
    Rename(String, String),
    Delete(String),
    Add(String, Vec<RepoId>),
    Remove(String, Vec<RepoId>),
    /// Runs the test operation on the group, confirmed when `true`.
    Run(String, bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The platform config dir when `None`.
    pub config_dir: Option<PathBuf>,
    pub action: Action,
}

/// Parses the arguments after `group`.
pub fn parse_args(args: &[OsString]) -> Option<Options> {
    let (config_dir, rest) = match args {
        [flag, dir, rest @ ..] if flag == "--config-dir" => (Some(PathBuf::from(dir)), rest),
        rest => (None, rest),
    };
    let text = |arg: &OsString| arg.to_str().map(str::to_owned);
    let repos = |args: &[OsString]| -> Option<Vec<RepoId>> {
        args.iter().map(|arg| text(arg).map(RepoId)).collect()
    };
    let action = match rest {
        [action] if action == "list" => Action::List,
        [action, name, members @ ..] if action == "create" => {
            Action::Create(text(name)?, repos(members)?)
        }
        [action, group, name] if action == "rename" => Action::Rename(text(group)?, text(name)?),
        [action, group] if action == "delete" => Action::Delete(text(group)?),
        [action, group, members @ ..] if action == "add" && !members.is_empty() => {
            Action::Add(text(group)?, repos(members)?)
        }
        [action, group, members @ ..] if action == "remove" && !members.is_empty() => {
            Action::Remove(text(group)?, repos(members)?)
        }
        [action, group, op] if action == "run" && op == "--test-op" => {
            Action::Run(text(group)?, false)
        }
        [action, group, op, confirm]
            if action == "run" && op == "--test-op" && confirm == "--confirm" =>
        {
            Action::Run(text(group)?, true)
        }
        _ => return None,
    };
    Some(Options { config_dir, action })
}

/// Prints the groups of the active workspace after the change, as JSON.
/// Exits 0 on success and 2 on any error, such as a refused name.
pub fn run(options: Options) -> ExitCode {
    if let Action::Run(group, confirm) = &options.action {
        return print(crate::group_run::run(
            options.config_dir.as_ref(),
            group,
            *confirm,
        ));
    }
    print(act(options))
}

fn print(result: Result<String, String>) -> ExitCode {
    match result {
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

pub(crate) fn load(config_dir: Option<&PathBuf>) -> Result<(Workspaces, WorkspaceId), String> {
    let workspaces = match config_dir {
        Some(dir) => Workspaces::load(&GivenConfigDir(dir.clone())),
        None => Workspaces::load(&PlatformConfigDir),
    };
    let id = workspaces
        .active()
        .ok_or_else(|| "no workspace is open".to_owned())?;
    Ok((workspaces, id))
}

/// The id of the group named `name`, ignoring case.
pub(crate) fn group(
    workspaces: &Workspaces,
    id: &WorkspaceId,
    name: &str,
) -> Result<GroupId, String> {
    let lower = name.trim().to_lowercase();
    workspaces
        .groups(id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|group| group.name.to_lowercase() == lower)
        .map(|group| group.id)
        .ok_or_else(|| format!("the workspace has no group named \"{name}\""))
}

fn act(options: Options) -> Result<String, String> {
    let (workspaces, id) = load(options.config_dir.as_ref())?;
    let changed = match &options.action {
        Action::List => Ok(()),
        Action::Create(name, repos) => workspaces.create_group(&id, name, repos).map(drop),
        Action::Rename(old, name) => {
            let group = group(&workspaces, &id, old)?;
            workspaces.rename_group(&id, &group, name)
        }
        Action::Delete(name) => {
            let group = group(&workspaces, &id, name)?;
            workspaces.delete_group(&id, &group)
        }
        Action::Add(name, repos) => {
            let group = group(&workspaces, &id, name)?;
            workspaces.add_to_group(&id, &group, repos, None)
        }
        Action::Remove(name, repos) => {
            let group = group(&workspaces, &id, name)?;
            workspaces.remove_from_group(&id, &group, repos)
        }
        Action::Run(..) => return Err("a run is not a change of the groups".to_owned()),
    };
    changed.map_err(|error| error.to_string())?;
    let groups = workspaces.groups(&id).map_err(|error| error.to_string())?;
    serde_json::to_string_pretty(&groups)
        .map_err(|error| format!("cannot write the result as JSON: {error}"))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeOptions {
    /// The platform config dir when `None`.
    pub config_dir: Option<PathBuf>,
    /// The platform cache dir when `None`.
    pub cache_dir: Option<PathBuf>,
}

/// Parses the arguments after `tree`, in any order.
pub fn parse_tree_args(args: &[OsString]) -> Option<TreeOptions> {
    let mut options = TreeOptions::default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if arg == "--config-dir" && options.config_dir.is_none() {
            options.config_dir = Some(PathBuf::from(rest.next()?));
        } else if arg == "--cache-dir" && options.cache_dir.is_none() {
            options.cache_dir = Some(PathBuf::from(rest.next()?));
        } else {
            return None;
        }
    }
    Some(options)
}

/// Prints the tree skeleton of the active workspace, with each group's
/// branch and totals from the cached summaries, as JSON.
pub fn run_tree(options: &TreeOptions) -> ExitCode {
    print(tree(options))
}

fn tree(options: &TreeOptions) -> Result<String, String> {
    let (workspaces, id) = load(options.config_dir.as_ref())?;
    let cache = match &options.cache_dir {
        Some(dir) => SummaryCache::open(&GivenCacheDir(dir.clone())),
        None => SummaryCache::open(&PlatformCacheDir),
    };
    let summaries = Summaries::load(cache, &workspaces);
    let nodes = Trees::new()
        .tree(&workspaces, &summaries, &id)
        .map_err(|error| error.to_string())?;
    serde_json::to_string_pretty(&nodes)
        .map_err(|error| format!("cannot write the result as JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    fn repos(ids: &[&str]) -> Vec<RepoId> {
        ids.iter().map(|id| RepoId((*id).to_owned())).collect()
    }

    #[test]
    fn grp_2_group_arguments() {
        let parsed = |list: &[&str]| parse_args(&args(list)).map(|options| options.action);
        assert_eq!(parsed(&["list"]), Some(Action::List));
        assert_eq!(
            parsed(&["create", "Backend", "r1", "r2"]),
            Some(Action::Create("Backend".to_owned(), repos(&["r1", "r2"])))
        );
        assert_eq!(
            parsed(&["create", "Empty"]),
            Some(Action::Create("Empty".to_owned(), Vec::new()))
        );
        assert_eq!(
            parsed(&["rename", "Backend", "Server"]),
            Some(Action::Rename("Backend".to_owned(), "Server".to_owned()))
        );
        assert_eq!(
            parsed(&["remove", "Backend", "r2"]),
            Some(Action::Remove("Backend".to_owned(), repos(&["r2"])))
        );
        assert_eq!(
            parsed(&["run", "Backend", "--test-op"]),
            Some(Action::Run("Backend".to_owned(), false))
        );
        assert_eq!(
            parsed(&["run", "Backend", "--test-op", "--confirm"]),
            Some(Action::Run("Backend".to_owned(), true))
        );
        assert_eq!(parsed(&["run", "Backend"]), None);
        assert_eq!(parsed(&["add", "Backend"]), None);
        assert_eq!(parsed(&["delete"]), None);
        let with_dir = parse_args(&args(&["--config-dir", "c", "list"])).unwrap();
        assert_eq!(with_dir.config_dir, Some(PathBuf::from("c")));
        assert_eq!(
            parse_tree_args(&args(&["--cache-dir", "k"])),
            Some(TreeOptions {
                config_dir: None,
                cache_dir: Some(PathBuf::from("k")),
            })
        );
        assert_eq!(parse_tree_args(&args(&["show"])), None);
    }
}
