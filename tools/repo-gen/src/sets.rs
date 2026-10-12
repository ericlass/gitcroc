//! The reference data sets of spec section 9 and their on-disk layout.
//!
//! A set lives in `<root>/<name>/`: one directory per repo under `repos/`, a
//! workspace file (section 10 format) when the set is a workspace, and
//! `repo-gen.json`, the manifest the bench harness reads. The manifest is
//! written last, so a set without one is incomplete.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

use crate::generate::{RepoSpec, generate_repo};
use crate::rng::hash_str;
use crate::{Error, git};

/// Bump when a change to the generator changes its output, so that existing
/// sets are rebuilt.
pub const GENERATOR_VERSION: u32 = 1;

/// The manifest file of a set.
pub const MANIFEST: &str = "repo-gen.json";

/// The workspace file of a set that is a workspace.
pub const WORKSPACE: &str = "workspace.toml";

/// Marks a directory as made by this generator, so it may be deleted and
/// rebuilt. Written before anything else.
const OWNED: &str = ".repo-gen";

/// Where `big-history` comes from: the Linux kernel at a fixed release tag,
/// with only that tag's history, so the data never changes.
pub const BIG_HISTORY_URL: &str = "https://github.com/torvalds/linux.git";
pub const BIG_HISTORY_TAG: &str = "v6.17";

/// The sets `just bench-data` knows, in the order `all` builds them.
pub const SET_NAMES: [&str; 4] = ["target-project", "big-tree", "many-repos", "big-history"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupSpec {
    pub name: String,
    pub repos: Vec<String>,
}

/// A generated set: its repos and, for a workspace, its groups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetSpec {
    pub name: String,
    pub repos: Vec<RepoSpec>,
    /// Written to a workspace file when not empty.
    pub groups: Vec<GroupSpec>,
}

/// `total` split into `parts` nearly equal shares; share `index` of them.
fn share(total: u64, parts: u64, index: u64) -> u64 {
    total * (index + 1) / parts - total * index / parts
}

fn groups_of(names: &[String], groups: usize) -> Vec<GroupSpec> {
    let size = names.len().div_ceil(groups);
    names
        .chunks(size)
        .enumerate()
        .map(|(index, chunk)| GroupSpec {
            name: format!("group-{}", index + 1),
            repos: chunk.to_vec(),
        })
        .collect()
}

/// `target-project` (section 9, gating): 20 repos in 4 groups, 200,000 files
/// and 2 million lines in total, one repo with 60,000 of the files, 5,000
/// commits and 30 branches per repo.
pub fn target_project() -> SetSpec {
    let mut repos = vec![RepoSpec {
        name: "repo-01".into(),
        files: 60_000,
        lines: 600_000,
        commits: 5_000,
        branches: 30,
    }];
    for index in 0..19 {
        repos.push(RepoSpec {
            name: format!("repo-{:02}", index + 2),
            files: u32::try_from(share(140_000, 19, index)).unwrap_or(u32::MAX),
            lines: share(1_400_000, 19, index),
            commits: 5_000,
            branches: 30,
        });
    }
    let names: Vec<String> = repos.iter().map(|repo| repo.name.clone()).collect();
    SetSpec {
        name: "target-project".into(),
        repos,
        groups: groups_of(&names, 4),
    }
}

/// `big-tree` (section 9, stretch): one repo with 500,000 tracked files.
/// Lines, commits and branches are not in the spec; they follow the density
/// of `target-project`.
pub fn big_tree() -> SetSpec {
    SetSpec {
        name: "big-tree".into(),
        repos: vec![RepoSpec {
            name: "big-tree".into(),
            files: 500_000,
            lines: 5_000_000,
            commits: 5_000,
            branches: 30,
        }],
        groups: Vec::new(),
    }
}

/// `many-repos` (section 9, stretch): 200 repos with 2,000 commits and 20
/// branches each, in 10 groups. Files and lines per repo are not in the spec;
/// 1,000 files and 10,000 lines each is assumed.
pub fn many_repos() -> SetSpec {
    let repos: Vec<RepoSpec> = (1..=200)
        .map(|index| RepoSpec {
            name: format!("repo-{index:03}"),
            files: 1_000,
            lines: 10_000,
            commits: 2_000,
            branches: 20,
        })
        .collect();
    let names: Vec<String> = repos.iter().map(|repo| repo.name.clone()).collect();
    SetSpec {
        name: "many-repos".into(),
        repos,
        groups: groups_of(&names, 10),
    }
}

/// One repo of a set, as the manifest lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestRepo {
    pub name: String,
    /// Relative to the set directory.
    pub path: PathBuf,
    /// Tracked files at `HEAD`, when known.
    pub files: Option<u32>,
}

/// `repo-gen.json`. The harness reads `repos`; the rest decides whether the
/// set is up to date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub generator_version: u32,
    pub set: String,
    /// The absolute set directory; the workspace file holds absolute paths.
    pub dir: PathBuf,
    /// For a generated set.
    pub spec: Option<SetSpec>,
    /// For a cloned set: URL and tag.
    pub source: Option<(String, String)>,
    pub repos: Vec<ManifestRepo>,
}

/// Whether a set was built or already up to date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Built,
    UpToDate,
}

/// The set directory under `root`, which is created if needed.
fn set_dir(root: &Path, name: &str) -> Result<PathBuf, Error> {
    std::fs::create_dir_all(root)?;
    Ok(std::path::absolute(root)?.join(name))
}

fn read_manifest(dir: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(dir.join(MANIFEST)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Empties `dir` for a rebuild. Only a directory this generator made is
/// deleted; anything else in the way is an error.
fn prepare(dir: &Path) -> Result<(), Error> {
    if dir.join(OWNED).is_file() {
        std::fs::remove_dir_all(dir)?;
    } else if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
        return Err(Error::Invalid(format!(
            "{} exists and was not made by repo-gen; move it away first",
            dir.display()
        )));
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(OWNED), "")?;
    Ok(())
}

/// Writes a file through a temporary file and a rename, so a reader never sees
/// half of it.
fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), Error> {
    let dir = path
        .parent()
        .ok_or_else(|| Error::Invalid(format!("{} has no parent", path.display())))?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut file, contents)?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[derive(Serialize)]
struct WorkspaceFile<'a> {
    schema: u32,
    name: &'a str,
    repo: Vec<WorkspaceRepo>,
    group: Vec<WorkspaceGroup<'a>>,
}

#[derive(Serialize)]
struct WorkspaceRepo {
    id: String,
    path: PathBuf,
}

#[derive(Serialize)]
struct WorkspaceGroup<'a> {
    name: &'a str,
    repos: Vec<String>,
}

/// The workspace file of section 10, with repo ids `r1`, `r2`, … in spec
/// order and absolute paths.
fn workspace_toml(spec: &SetSpec, dir: &Path) -> Result<String, Error> {
    let id = |name: &str| {
        spec.repos
            .iter()
            .position(|repo| repo.name == name)
            .map(|index| format!("r{}", index + 1))
            .ok_or_else(|| Error::Invalid(format!("group lists unknown repo {name}")))
    };
    let file = WorkspaceFile {
        schema: 1,
        name: &spec.name,
        repo: spec
            .repos
            .iter()
            .enumerate()
            .map(|(index, repo)| WorkspaceRepo {
                id: format!("r{}", index + 1),
                path: dir.join("repos").join(&repo.name),
            })
            .collect(),
        group: spec
            .groups
            .iter()
            .map(|group| {
                Ok(WorkspaceGroup {
                    name: &group.name,
                    repos: group
                        .repos
                        .iter()
                        .map(|name| id(name))
                        .collect::<Result<_, _>>()?,
                })
            })
            .collect::<Result<_, Error>>()?,
    };
    Ok(toml::to_string(&file)?)
}

/// Builds a generated set under `root`, unless the one there was built by
/// this generator version from the same spec at the same place. Repos are
/// built in parallel; `progress` is called with each finished repo's name.
pub fn generate_set(
    spec: &SetSpec,
    root: &Path,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<Outcome, Error> {
    let dir = set_dir(root, &spec.name)?;
    if let Some(manifest) = read_manifest(&dir)
        && manifest.generator_version == GENERATOR_VERSION
        && manifest.spec.as_ref() == Some(spec)
        && manifest.dir == dir
    {
        return Ok(Outcome::UpToDate);
    }
    prepare(&dir)?;

    let next = AtomicUsize::new(0);
    let failure: Mutex<Option<Error>> = Mutex::new(None);
    let workers = std::thread::available_parallelism()
        .map_or(4, usize::from)
        .min(spec.repos.len());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(repo) = spec.repos.get(index) else {
                        break;
                    };
                    let seed = hash_str(&spec.name) ^ hash_str(&repo.name).rotate_left(17);
                    match generate_repo(repo, seed, &dir.join("repos").join(&repo.name)) {
                        Ok(()) => progress(&repo.name),
                        Err(error) => {
                            if let Ok(mut slot) = failure.lock() {
                                slot.get_or_insert(error);
                            }
                            // Stop the other workers too.
                            next.store(usize::MAX / 2, Ordering::Relaxed);
                            break;
                        }
                    }
                }
            });
        }
    });
    if let Some(error) = failure.into_inner().ok().flatten() {
        return Err(error);
    }

    if !spec.groups.is_empty() {
        write_atomic(&dir.join(WORKSPACE), workspace_toml(spec, &dir)?.as_bytes())?;
    }
    let manifest = Manifest {
        generator_version: GENERATOR_VERSION,
        set: spec.name.clone(),
        dir: dir.clone(),
        spec: Some(spec.clone()),
        source: None,
        repos: spec
            .repos
            .iter()
            .map(|repo| ManifestRepo {
                name: repo.name.clone(),
                path: Path::new("repos").join(&repo.name),
                files: Some(repo.files),
            })
            .collect(),
    };
    write_atomic(&dir.join(MANIFEST), &serde_json::to_vec_pretty(&manifest)?)?;
    Ok(Outcome::Built)
}

/// `big-history`: clones the Linux kernel at [`BIG_HISTORY_TAG`] with that
/// tag's history only, then writes its commit-graph file. Needs the network
/// and several GB of disk; git shows its own progress.
pub fn clone_big_history(root: &Path) -> Result<Outcome, Error> {
    let dir = set_dir(root, "big-history")?;
    let source = (BIG_HISTORY_URL.to_owned(), BIG_HISTORY_TAG.to_owned());
    if let Some(manifest) = read_manifest(&dir)
        && manifest.generator_version == GENERATOR_VERSION
        && manifest.source.as_ref() == Some(&source)
        && manifest.dir == dir
    {
        return Ok(Outcome::UpToDate);
    }
    prepare(&dir)?;

    // The user's own git config stays in effect here (proxies, for example).
    let mut clone = std::process::Command::new("git");
    clone
        .args([
            "clone",
            "--single-branch",
            "--branch",
            BIG_HISTORY_TAG,
            "--",
        ])
        .args([BIG_HISTORY_URL, "linux"])
        .current_dir(&dir)
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0");
    let status = clone.status()?;
    if !status.success() {
        return Err(Error::Git {
            args: git::describe(&clone),
            code: status.code(),
            stderr: String::new(),
        });
    }
    git::run(git::command(
        &dir.join("linux"),
        ["commit-graph", "write", "--reachable"],
    ))?;

    let manifest = Manifest {
        generator_version: GENERATOR_VERSION,
        set: "big-history".into(),
        dir: dir.clone(),
        spec: None,
        source: Some(source),
        repos: vec![ManifestRepo {
            name: "linux".into(),
            path: PathBuf::from("linux"),
            files: None,
        }],
    };
    write_atomic(&dir.join(MANIFEST), &serde_json::to_vec_pretty(&manifest)?)?;
    Ok(Outcome::Built)
}
