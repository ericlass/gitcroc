//! `gitcroc-cli summaries`: the summary of every repo of the workspace, as
//! JSON (PERF-3). With `--cached-only` only the cache is read, as at the
//! start of the app; otherwise every repo is read again first.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use gitcroc_core::{
    CacheDir, GitRunner, GitSetup, PlatformCacheDir, PlatformConfigDir, Summaries, SummaryCache,
    Workspaces, refresh_summaries,
};

use crate::workspace::GivenConfigDir;

pub const USAGE: &str = "summaries [--config-dir <dir>] [--cache-dir <dir>] [--cached-only]";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// The platform config dir when `None`.
    pub config_dir: Option<PathBuf>,
    /// The platform cache dir when `None`.
    pub cache_dir: Option<PathBuf>,
    pub cached_only: bool,
}

/// Parses the arguments after `summaries`, in any order.
pub fn parse_args(args: &[OsString]) -> Option<Options> {
    let mut options = Options::default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if arg == "--config-dir" && options.config_dir.is_none() {
            options.config_dir = Some(PathBuf::from(rest.next()?));
        } else if arg == "--cache-dir" && options.cache_dir.is_none() {
            options.cache_dir = Some(PathBuf::from(rest.next()?));
        } else if arg == "--cached-only" && !options.cached_only {
            options.cached_only = true;
        } else {
            return None;
        }
    }
    Some(options)
}

/// The cache dir given on the command line.
pub(crate) struct GivenCacheDir(pub PathBuf);

impl CacheDir for GivenCacheDir {
    fn cache_dir(&self) -> Option<PathBuf> {
        Some(self.0.clone())
    }
}

/// Prints the summaries as a JSON array in workspace order. Exits 0 on
/// success and 2 on any error, such as no usable git for a refresh.
pub fn run(options: Options) -> ExitCode {
    match summaries(options) {
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

fn summaries(options: Options) -> Result<String, String> {
    let workspaces = Arc::new(match &options.config_dir {
        Some(dir) => Workspaces::load(&GivenConfigDir(dir.clone())),
        None => Workspaces::load(&PlatformConfigDir),
    });
    let cache = match &options.cache_dir {
        Some(dir) => SummaryCache::open(&GivenCacheDir(dir.clone())),
        None => SummaryCache::open(&PlatformCacheDir),
    };
    let summaries = Arc::new(Summaries::load(cache, &workspaces));
    let workspace = workspaces
        .active()
        .and_then(|id| workspaces.view(&id).ok())
        .ok_or_else(|| "no workspace is open".to_owned())?;
    if !options.cached_only {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("cannot start the async runtime: {error}"))?;
        let setup = GitSetup::new();
        let status = runtime.block_on(setup.detect());
        let git = runtime
            .block_on(setup.git())
            .ok_or_else(|| format!("no usable git: {status:?}"))?;
        let repos = workspace.repos.iter().map(|repo| repo.id.clone()).collect();
        runtime.block_on(refresh_summaries(
            workspaces.clone(),
            summaries.clone(),
            workspace.id.clone(),
            repos,
            GitRunner::new(git.path),
            |_| {},
        ));
    }
    let views = summaries
        .views(&workspaces, &workspace.id)
        .map_err(|error| error.to_string())?;
    serde_json::to_string_pretty(&views)
        .map_err(|error| format!("cannot write the result as JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perf_3_summaries_arguments() {
        let args = |list: &[&str]| -> Vec<OsString> { list.iter().map(OsString::from).collect() };
        assert_eq!(parse_args(&args(&[])), Some(Options::default()));
        assert_eq!(
            parse_args(&args(&[
                "--cached-only",
                "--cache-dir",
                "k",
                "--config-dir",
                "c"
            ])),
            Some(Options {
                config_dir: Some(PathBuf::from("c")),
                cache_dir: Some(PathBuf::from("k")),
                cached_only: true,
            })
        );
        assert_eq!(parse_args(&args(&["--cache-dir"])), None);
        assert_eq!(parse_args(&args(&["--cached-only", "--cached-only"])), None);
        assert_eq!(parse_args(&args(&["show"])), None);
    }
}
