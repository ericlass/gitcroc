//! `gitcroc-cli history`: pages of a repo's history as JSON (PERF-2), or of
//! the merged history of a group or several repos of the active workspace
//! (PERF-5), each row with its repo's id.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitcroc_core::{
    Filter, GitRepo, HistoryStart, RepoId, TargetSet, history_page, merged_history, target_repos,
};
use serde_json::{Value, json};

pub const USAGE: &str = "history <repo-path> [--ref <name>] [--filter <text>] [--pages <n>] | \
history [--config-dir <dir>] --group <name>|--repos <repo-id>,... [--filter <text>] [--pages <n>]";

/// Whose history.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    Repo {
        path: PathBuf,
        start: HistoryStart,
    },
    /// A group of the active workspace, by name, ignoring case.
    Group(String),
    Repos(Vec<RepoId>),
}

pub struct Options {
    source: Source,
    /// The platform config dir when `None`; only a merged history reads it.
    config_dir: Option<PathBuf>,
    filter: Option<Filter>,
    pages: usize,
}

/// Parses the arguments after `history`.
pub fn parse_args(args: &[OsString]) -> Option<Options> {
    let mut path = None;
    let mut start = None;
    let mut merged = None;
    let mut config_dir = None;
    let mut filter = None;
    let mut pages = 1;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = || rest.next().and_then(|value| value.to_str());
        match arg.to_str()? {
            "--ref" => start = Some(HistoryStart::Ref(value()?.to_owned())),
            "--filter" => filter = Some(Filter::new(value()?)),
            "--pages" => pages = value()?.parse().ok().filter(|&n| n > 0)?,
            "--config-dir" => config_dir = Some(PathBuf::from(value()?)),
            "--group" if merged.is_none() => merged = Some(Source::Group(value()?.to_owned())),
            "--repos" if merged.is_none() => {
                let ids: Vec<RepoId> = value()?
                    .split(',')
                    .map(|id| RepoId(id.trim().to_owned()))
                    .collect();
                if ids.iter().any(|id| id.0.is_empty()) {
                    return None;
                }
                merged = Some(Source::Repos(ids));
            }
            flag if flag.starts_with("--") => return None,
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return None,
        }
    }
    let source = match (path, merged) {
        (Some(path), None) if config_dir.is_none() => Source::Repo {
            path,
            start: start.unwrap_or(HistoryStart::All),
        },
        (None, Some(merged)) if start.is_none() => merged,
        _ => return None,
    };
    Some(Options {
        source,
        config_dir,
        filter,
        pages,
    })
}

/// Prints a JSON array with one object per page. Exits 0 on success and 2
/// when the repo, group or workspace cannot be read.
pub fn run(options: Options) -> ExitCode {
    let result = match &options.source {
        Source::Repo { path, start } => read(&options, path, start),
        Source::Group(_) | Source::Repos(_) => read_merged(&options),
    };
    match result.and_then(|pages| {
        serde_json::to_string_pretty(&pages)
            .map_err(|error| format!("cannot write the result as JSON: {error}"))
    }) {
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

fn read(options: &Options, path: &Path, start: &HistoryStart) -> Result<Vec<Value>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|error| format!("cannot start the async runtime: {error}"))?;
    let repo = GitRepo::open(path).map_err(|error| error.to_string())?;
    let mut pages = Vec::new();
    let mut cursor = None;
    for number in 1..=options.pages {
        let page = runtime
            .block_on(history_page(
                repo.clone(),
                start.clone(),
                cursor,
                options.filter.clone(),
            ))
            .map_err(|error| error.to_string())?;
        cursor = page.next.clone();
        let more = cursor.is_some();
        pages.push(json!({ "page": number, "more": more, "rows": page.rows }));
        if !more {
            break;
        }
    }
    Ok(pages)
}

/// The merged history of the target set in the active workspace (GRP-5).
fn read_merged(options: &Options) -> Result<Vec<Value>, String> {
    let (workspaces, id) = crate::group::load(options.config_dir.as_ref())?;
    let target = match &options.source {
        Source::Group(name) => TargetSet::Group {
            group_id: crate::group::group(&workspaces, &id, name)?,
        },
        Source::Repos(ids) => TargetSet::Repos {
            repo_ids: ids.clone(),
        },
        Source::Repo { .. } => return Err("not a merged history".into()),
    };
    let repos = target_repos(&workspaces, &id, &target).map_err(|error| error.to_string())?;
    let paths = repos
        .iter()
        .map(|repo| workspaces.repo_path(&id, repo))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let mut pages = Vec::new();
    let mut cursor = None;
    for number in 1..=options.pages {
        let page = merged_history(&paths, cursor, options.filter.clone())
            .map_err(|error| error.to_string())?;
        cursor = page.next;
        let more = cursor.is_some();
        let rows: Vec<Value> = page
            .rows
            .into_iter()
            .map(|row| {
                let mut value = json!(row.row);
                if let (Some(object), Some(repo)) = (value.as_object_mut(), repos.get(row.repo)) {
                    object.insert("repo_id".into(), json!(repo));
                }
                value
            })
            .collect();
        pages.push(json!({ "page": number, "more": more, "rows": rows }));
        if !more {
            break;
        }
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn perf_2_history_arguments() {
        let options = parse_args(&args(&["repo", "--ref", "main", "--pages", "3"])).unwrap();
        assert_eq!(
            options.source,
            Source::Repo {
                path: PathBuf::from("repo"),
                start: HistoryStart::Ref("main".into())
            }
        );
        assert_eq!((options.pages, options.filter), (3, None));
        let options = parse_args(&args(&["repo", "--filter", "Fix"])).unwrap();
        assert_eq!(options.filter, Some(Filter::new("fix")));
        assert!(parse_args(&args(&[])).is_none());
        assert!(parse_args(&args(&["repo", "--pages", "0"])).is_none());
        assert!(parse_args(&args(&["repo", "--ref"])).is_none());
    }

    #[test]
    fn perf_5_merged_history_arguments() {
        let options = parse_args(&args(&["--group", "Backend", "--config-dir", "c"])).unwrap();
        assert_eq!(options.source, Source::Group("Backend".into()));
        assert_eq!(options.config_dir, Some(PathBuf::from("c")));
        let options = parse_args(&args(&["--repos", "r1,r2", "--pages", "2"])).unwrap();
        assert_eq!(
            options.source,
            Source::Repos(vec![RepoId("r1".into()), RepoId("r2".into())])
        );
        // One source only, and a ref only for a single repo.
        assert!(parse_args(&args(&["repo", "--group", "Backend"])).is_none());
        assert!(parse_args(&args(&["--group", "a", "--repos", "r1"])).is_none());
        assert!(parse_args(&args(&["--group", "a", "--ref", "main"])).is_none());
        assert!(parse_args(&args(&["--repos", "r1,,r2"])).is_none());
        assert!(parse_args(&args(&["repo", "--config-dir", "c"])).is_none());
    }
}
