// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use repo_gen::{GroupSpec, Outcome, Repo, RepoSpec, SetSpec};

fn small_set() -> SetSpec {
    let repo = |name: &str, files| RepoSpec {
        name: name.into(),
        files,
        lines: u64::from(files) * 10,
        commits: 120,
        branches: 5,
    };
    SetSpec {
        name: "small".into(),
        repos: vec![repo("a", 300), repo("b", 150)],
        groups: vec![GroupSpec {
            name: "all".into(),
            repos: vec!["a".into(), "b".into()],
        }],
    }
}

fn refs(repo: &Path) -> String {
    Repo::open(repo)
        .git(["for-each-ref", "--format=%(objectname) %(refname)"])
        .unwrap()
}

#[test]
fn generating_twice_gives_identical_commit_ids() {
    let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let set = small_set();
    for root in [first.path(), second.path()] {
        let outcome = repo_gen::generate_set(&set, root, &|_| {}).unwrap();
        assert_eq!(outcome, Outcome::Built);
    }
    for repo in &set.repos {
        let path = |root: &Path| root.join("small/repos").join(&repo.name);
        let ids = refs(&path(first.path()));
        assert!(ids.lines().count() >= 5, "{ids}");
        assert_eq!(ids, refs(&path(second.path())), "repo {}", repo.name);
    }
}

#[test]
fn generated_repo_matches_its_spec() {
    let root = tempfile::tempdir().unwrap();
    let set = small_set();
    repo_gen::generate_set(&set, root.path(), &|_| {}).unwrap();
    let spec = &set.repos[0];
    let repo = Repo::open(root.path().join("small/repos/a"));
    let count = |args: &[&str]| repo.git(args).unwrap().trim().parse::<u64>().unwrap();

    assert_eq!(
        count(&["rev-list", "--all", "--count"]),
        u64::from(spec.commits)
    );
    assert!(count(&["rev-list", "--all", "--merges", "--count"]) > 0);
    let branches = repo.git(["branch", "--format=%(refname)"]).unwrap();
    assert_eq!(branches.lines().count(), spec.branches as usize);
    assert_eq!(
        repo.git(["symbolic-ref", "HEAD"]).unwrap().trim(),
        "refs/heads/main"
    );

    // Every file is checked out and the lines add up.
    let files = repo.git(["ls-files", "-z"]).unwrap();
    let files: Vec<&str> = files.split('\0').filter(|f| !f.is_empty()).collect();
    assert_eq!(files.len(), spec.files as usize);
    let lines: usize = files
        .iter()
        .map(|file| {
            let text = std::fs::read_to_string(repo.path().join(file)).unwrap();
            text.lines().count()
        })
        .sum();
    assert_eq!(lines as u64, spec.lines);
    assert!(repo.git(["status", "--porcelain"]).unwrap().is_empty());
    assert!(repo.path().join(".git/objects/info/commit-graph").is_file());
}

#[test]
fn second_run_is_a_no_op_and_workspace_file_lists_the_repos() {
    let root = tempfile::tempdir().unwrap();
    let set = small_set();
    assert_eq!(
        repo_gen::generate_set(&set, root.path(), &|_| {}).unwrap(),
        Outcome::Built
    );
    let manifest = root.path().join("small").join(repo_gen::MANIFEST);
    let before = std::fs::metadata(&manifest).unwrap().modified().unwrap();
    assert_eq!(
        repo_gen::generate_set(&set, root.path(), &|_| {}).unwrap(),
        Outcome::UpToDate
    );
    assert_eq!(
        std::fs::metadata(&manifest).unwrap().modified().unwrap(),
        before
    );

    let text =
        std::fs::read_to_string(root.path().join("small").join(repo_gen::WORKSPACE)).unwrap();
    let workspace: toml::Table = text.parse().unwrap();
    assert_eq!(workspace["schema"].as_integer(), Some(1));
    let repos = workspace["repo"].as_array().unwrap();
    assert_eq!(repos.len(), 2);
    assert_eq!(repos[0]["id"].as_str(), Some("r1"));
    let path = Path::new(repos[1]["path"].as_str().unwrap());
    assert!(path.is_absolute() && path.join(".git").is_dir(), "{path:?}");
    let group = &workspace["group"].as_array().unwrap()[0];
    assert_eq!(group["repos"].as_array().unwrap().len(), 2);

    // A changed spec rebuilds the set.
    let mut changed = set.clone();
    changed.repos[1].commits += 4;
    assert_eq!(
        repo_gen::generate_set(&changed, root.path(), &|_| {}).unwrap(),
        Outcome::Built
    );
}

#[test]
fn a_directory_not_made_by_the_generator_is_left_alone() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("small");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("precious.txt"), "keep").unwrap();
    assert!(repo_gen::generate_set(&small_set(), root.path(), &|_| {}).is_err());
    assert!(dir.join("precious.txt").is_file());
}
