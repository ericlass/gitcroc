//! `gitcroc-cli group`: list, create, rename and delete the groups of the
//! active workspace, and add and remove their repos (GRP-2, GRP-3), and run
//! a group operation or the test operation on a group (`group_run`). Groups are named by name,
//! ignoring case, as their ids last one run only. Also
//! `gitcroc-cli tree`: the active workspace's tree skeleton (UI-1). Both
//! print JSON.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use gitcroc_core::{
    GroupId, GroupOp, MissingBranch, PlatformCacheDir, PlatformConfigDir, Prune, PullStrategy,
    RepoId, StartPoint, Summaries, SummaryCache, Trees, WorkspaceId, Workspaces,
};

use crate::summaries::GivenCacheDir;
use crate::workspace::GivenConfigDir;

pub const USAGE: &str = "group [--config-dir <dir>] list|create <name> [<repo-id>...]|\
rename <group> <name>|delete <group>|add <group> <repo-id>...|remove <group> <repo-id>...|\
run <group> --test-op|fetch [--prune|--no-prune]|\
pull [--strategy config|ff-only|merge|rebase]|push [--no-set-upstream] [--force-with-lease]|\
checkout <branch> [--missing skip|default-branch|create]|\
create-branch <branch> [--start <ref>|--start-default] [--no-switch]|\
delete-branch <branch> [--force] [--on-remote] [--confirm]";

pub const TREE_USAGE: &str = "tree [--config-dir <dir>] [--cache-dir <dir>]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    List,
    Create(String, Vec<RepoId>),
    Rename(String, String),
    Delete(String),
    Add(String, Vec<RepoId>),
    Remove(String, Vec<RepoId>),
    /// Runs an operation on the group, confirmed when `true`.
    Run(String, RunOp, bool),
}

/// What `group run` runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOp {
    /// The test operation of M3-08.
    Test,
    Op(GroupOp),
}

/// Parses the operation and its options after `run <group>`, then
/// `--confirm`.
fn parse_run(args: &[OsString]) -> Option<(RunOp, bool)> {
    let (args, confirm) = match args {
        [rest @ .., last] if last == "--confirm" => (rest, true),
        rest => (rest, false),
    };
    let (op, options) = args.split_first()?;
    let flags: Vec<&str> = options
        .iter()
        .map(|arg| arg.to_str())
        .collect::<Option<_>>()?;
    let op = match (op.to_str()?, flags.as_slice()) {
        ("--test-op", []) => RunOp::Test,
        ("fetch", flags) => RunOp::Op(GroupOp::Fetch {
            prune: match flags {
                [] => Prune::Config,
                ["--prune"] => Prune::Prune,
                ["--no-prune"] => Prune::NoPrune,
                _ => return None,
            },
        }),
        ("pull", flags) => RunOp::Op(GroupOp::Pull {
            strategy: match flags {
                [] | ["--strategy", "config"] => PullStrategy::Config,
                ["--strategy", "ff-only"] => PullStrategy::FfOnly,
                ["--strategy", "merge"] => PullStrategy::Merge,
                ["--strategy", "rebase"] => PullStrategy::Rebase,
                _ => return None,
            },
        }),
        ("push", flags) => {
            let known = ["--no-set-upstream", "--force-with-lease"];
            if !flags.iter().all(|flag| known.contains(flag)) {
                return None;
            }
            RunOp::Op(GroupOp::Push {
                set_upstream: !flags.contains(&"--no-set-upstream"),
                force_with_lease: flags.contains(&"--force-with-lease"),
            })
        }
        ("checkout", [name, rest @ ..]) => RunOp::Op(GroupOp::Checkout {
            name: (*name).to_owned(),
            missing: match rest {
                [] | ["--missing", "skip"] => MissingBranch::Skip,
                ["--missing", "default-branch"] => MissingBranch::DefaultBranch,
                ["--missing", "create"] => MissingBranch::Create,
                _ => return None,
            },
        }),
        ("create-branch", [name, rest @ ..]) => {
            let (start, rest) = match rest {
                ["--start", revision, rest @ ..] => (
                    StartPoint::Revision {
                        revision: (*revision).to_owned(),
                    },
                    rest,
                ),
                ["--start-default", rest @ ..] => (StartPoint::DefaultBranch, rest),
                rest => (StartPoint::Head, rest),
            };
            let switch = match rest {
                [] => true,
                ["--no-switch"] => false,
                _ => return None,
            };
            RunOp::Op(GroupOp::CreateBranch {
                name: (*name).to_owned(),
                start,
                switch,
            })
        }
        ("delete-branch", [name, rest @ ..]) => {
            let known = ["--force", "--on-remote"];
            if !rest.iter().all(|flag| known.contains(flag)) {
                return None;
            }
            RunOp::Op(GroupOp::DeleteBranch {
                name: (*name).to_owned(),
                force: rest.contains(&"--force"),
                on_remote: rest.contains(&"--on-remote"),
            })
        }
        _ => return None,
    };
    Some((op, confirm))
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
        [action, group, rest @ ..] if action == "run" => {
            let (op, confirm) = parse_run(rest)?;
            Action::Run(text(group)?, op, confirm)
        }
        _ => return None,
    };
    Some(Options { config_dir, action })
}

/// Prints the groups of the active workspace after the change, as JSON.
/// Exits 0 on success and 2 on any error, such as a refused name.
pub fn run(options: Options) -> ExitCode {
    if let Action::Run(group, op, confirm) = &options.action {
        return print(crate::group_run::run(
            options.config_dir.as_ref(),
            group,
            op,
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
        let run = |op: RunOp, confirm: bool| Some(Action::Run("Backend".to_owned(), op, confirm));
        assert_eq!(
            parsed(&["run", "Backend", "--test-op"]),
            run(RunOp::Test, false)
        );
        assert_eq!(
            parsed(&["run", "Backend", "--test-op", "--confirm"]),
            run(RunOp::Test, true)
        );
        assert_eq!(parsed(&["run", "Backend"]), None);
        assert_eq!(
            parsed(&["run", "Backend", "fetch", "--no-prune"]),
            run(
                RunOp::Op(GroupOp::Fetch {
                    prune: Prune::NoPrune
                }),
                false
            )
        );
        assert_eq!(
            parsed(&["run", "Backend", "pull", "--strategy", "merge"]),
            run(
                RunOp::Op(GroupOp::Pull {
                    strategy: PullStrategy::Merge
                }),
                false
            )
        );
        assert_eq!(
            parsed(&["run", "Backend", "push", "--force-with-lease", "--confirm"]),
            run(
                RunOp::Op(GroupOp::Push {
                    set_upstream: true,
                    force_with_lease: true
                }),
                true
            )
        );
        assert_eq!(
            parsed(&["run", "Backend", "pull", "--strategy", "squash"]),
            None
        );
        assert_eq!(parsed(&["run", "Backend", "push", "--prune"]), None);
        assert_eq!(
            parsed(&[
                "run",
                "Backend",
                "checkout",
                "main",
                "--missing",
                "default-branch"
            ]),
            run(
                RunOp::Op(GroupOp::Checkout {
                    name: "main".to_owned(),
                    missing: MissingBranch::DefaultBranch
                }),
                false
            )
        );
        assert_eq!(
            parsed(&[
                "run",
                "Backend",
                "create-branch",
                "x",
                "--start",
                "v1",
                "--no-switch"
            ]),
            run(
                RunOp::Op(GroupOp::CreateBranch {
                    name: "x".to_owned(),
                    start: StartPoint::Revision {
                        revision: "v1".to_owned()
                    },
                    switch: false
                }),
                false
            )
        );
        assert_eq!(
            parsed(&[
                "run",
                "Backend",
                "delete-branch",
                "x",
                "--on-remote",
                "--confirm"
            ]),
            run(
                RunOp::Op(GroupOp::DeleteBranch {
                    name: "x".to_owned(),
                    force: false,
                    on_remote: true
                }),
                true
            )
        );
        assert_eq!(parsed(&["run", "Backend", "checkout"]), None);
        assert_eq!(
            parsed(&["run", "Backend", "delete-branch", "x", "--prune"]),
            None
        );
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
