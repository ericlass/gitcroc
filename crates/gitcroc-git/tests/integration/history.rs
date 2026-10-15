//! History pages on real repos (PERF-2, ARCH-8, UI-9, UI-12, SEC-3), built
//! with the fixture helpers and the generator of M1-01.

use std::collections::{HashMap, HashSet};

use gitcroc_git::{
    CommitRow, Filter, GitRepo, HistoryError, HistoryStart, PAGE_SIZE, Page, RefKind, RepoRead,
};
use repo_gen::{Fixture, Repo, RepoSpec};

/// Every page from `start`, first to last.
fn all_pages(repo: &GitRepo, start: &HistoryStart, filter: Option<&Filter>) -> Vec<Page> {
    let mut pages = Vec::new();
    let mut cursor = None;
    loop {
        let page = repo
            .history(start.clone(), cursor, filter.cloned())
            .unwrap();
        cursor = page.next.clone();
        pages.push(page);
        if cursor.is_none() {
            return pages;
        }
    }
}

fn rows(pages: &[Page]) -> Vec<&CommitRow> {
    pages.iter().flat_map(|page| &page.rows).collect()
}

/// No commit is listed after one of its parents.
fn assert_children_first(rows: &[&CommitRow]) {
    let position: HashMap<&str, usize> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.hash.as_str(), index))
        .collect();
    for (index, row) in rows.iter().enumerate() {
        for parent in &row.parents {
            if let Some(&at) = position.get(parent.as_str()) {
                assert!(
                    index < at,
                    "{} is listed after its parent {parent}",
                    row.hash
                );
            }
        }
    }
}

/// A generated repo with `commits` commits and a commit-graph file.
fn generated(commits: u32) -> (tempfile::TempDir, GitRepo) {
    let dir = tempfile::tempdir().unwrap();
    let spec = RepoSpec {
        name: "history".into(),
        files: 4,
        lines: 40,
        commits,
        branches: 8,
    };
    repo_gen::generate_repo(&spec, 7, dir.path()).unwrap();
    let repo = GitRepo::open(dir.path()).unwrap();
    (dir, repo)
}

fn empty_commit(repo: &Repo, message: &str) -> String {
    repo.git(["commit", "--quiet", "--allow-empty", "-m", message])
        .unwrap();
    repo.head().unwrap()
}

fn write_commit_graph(repo: &Repo) {
    repo.git(["commit-graph", "write", "--reachable"]).unwrap();
}

#[test]
fn perf_2_pages_continue_without_walking_everything() {
    let (dir, repo) = generated(1_200);
    let start = HistoryStart::All;

    let first = repo.history(start.clone(), None, None).unwrap();
    let cursor = first.next.as_ref().unwrap();
    assert!(
        cursor.commits_read() <= 2 * PAGE_SIZE as u64,
        "page one read {} of 1,200 commits",
        cursor.commits_read()
    );

    let pages = all_pages(&repo, &start, None);
    let sizes: Vec<usize> = pages.iter().map(|page| page.rows.len()).collect();
    assert_eq!(sizes, [500, 500, 200]);
    let all = rows(&pages);
    let listed: HashSet<&str> = all.iter().map(|row| row.hash.as_str()).collect();
    assert_eq!(listed.len(), 1_200, "no commit is listed twice");
    let expected = Repo::open(dir.path()).git(["rev-list", "--all"]).unwrap();
    let expected: HashSet<&str> = expected.lines().collect();
    assert_eq!(listed, expected, "no commit is left out");
    assert_children_first(&all);
    assert_eq!(all[..500], rows(&[first])[..]);
}

#[test]
fn arch_8_history_page_is_at_most_500_commits() {
    let (_dir, repo) = generated(1_200);
    for filter in [None, Some(Filter::new(""))] {
        let pages = all_pages(&repo, &HistoryStart::All, filter.as_ref());
        assert!(pages.iter().all(|page| page.rows.len() <= PAGE_SIZE));
        assert_eq!(rows(&pages).len(), 1_200);
    }
}

#[test]
fn perf_2_lanes_continue_across_pages() {
    // side forks from base and is merged at the top; 520 commits on main in
    // between put the fork on page two.
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    empty_commit(&git, "base");
    git.branch("side").unwrap();
    git.switch("side").unwrap();
    let side = empty_commit(&git, "side");
    git.switch("main").unwrap();
    let messages: Vec<String> = (0..520).map(|n| format!("main {n}")).collect();
    git.empty_commits(&messages).unwrap();
    git.merge("side", "Merge side").unwrap();
    write_commit_graph(&git);

    let repo = GitRepo::open(git.path()).unwrap();
    let pages = all_pages(&repo, &HistoryStart::Ref("main".into()), None);
    assert_eq!(pages.len(), 2);
    let merge = pages[0].rows[0].graph.clone().unwrap();
    let side_line = merge
        .down
        .iter()
        .find(|edge| edge.to != merge.lane)
        .unwrap();
    let last = pages[0].rows.last().unwrap().graph.clone().unwrap();
    assert!(
        last.down.contains(&gitcroc_git::Edge {
            from: side_line.to,
            to: side_line.to,
            color: side_line.color,
        }),
        "the side line leaves page one: {last:?}"
    );
    let side_row = pages[1].rows.iter().find(|row| row.hash == side).unwrap();
    let side_cell = side_row.graph.clone().unwrap();
    assert_eq!(
        (side_cell.lane, side_cell.color),
        (side_line.to, side_line.color)
    );
}

#[test]
fn perf_2_a_child_is_never_listed_after_its_parent() {
    // Each branch commits with a clock set back below its parent's date.
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("skew").unwrap();
    git.set_clock(1_000);
    empty_commit(&git, "root");
    git.branch("side").unwrap();
    git.set_clock(10);
    empty_commit(&git, "older than root");
    git.set_clock(2_000);
    empty_commit(&git, "newer");
    git.switch("side").unwrap();
    git.set_clock(5);
    empty_commit(&git, "side, oldest");
    git.set_clock(3_000);
    git.switch("main").unwrap();
    git.merge("side", "Merge side").unwrap();

    let repo = GitRepo::open(git.path()).unwrap();
    for with_graph in [false, true] {
        if with_graph {
            write_commit_graph(&git);
        }
        let pages = all_pages(&repo, &HistoryStart::All, None);
        let rows = rows(&pages);
        assert_eq!(rows.len(), 5);
        assert_children_first(&rows);
    }
}

#[test]
fn ui_9_ref_labels() {
    let fixture = Fixture::new().unwrap();
    let upstream = fixture.repo("upstream").unwrap();
    let first = empty_commit(&upstream, "first");
    let git = upstream.clone_to(fixture.path().join("clone")).unwrap();
    let second = empty_commit(&git, "second");
    git.annotated_tag("v1.0", "release").unwrap();
    git.branch("feature").unwrap();

    let labels = |repo: &GitRepo| -> HashMap<String, Vec<(RefKind, String)>> {
        let pages = all_pages(repo, &HistoryStart::All, None);
        rows(&pages)
            .into_iter()
            .map(|row| {
                let refs = row.refs.iter().map(|r| (r.kind, r.name.clone())).collect();
                (row.hash.clone(), refs)
            })
            .collect()
    };
    let repo = GitRepo::open(git.path()).unwrap();
    let found = labels(&repo);
    assert_eq!(
        found[&second],
        [
            (RefKind::CurrentBranch, "main".into()),
            (RefKind::LocalBranch, "feature".into()),
            (RefKind::Tag, "v1.0".into()),
        ]
    );
    assert_eq!(
        found[&first],
        [(RefKind::RemoteBranch, "origin/main".into())]
    );

    git.git(["switch", "--quiet", "--detach", "HEAD~1"])
        .unwrap();
    let found = labels(&repo);
    assert_eq!(
        found[&first],
        [
            (RefKind::Head, "HEAD".into()),
            (RefKind::RemoteBranch, "origin/main".into()),
        ]
    );
    assert_eq!(found[&second][0], (RefKind::LocalBranch, "feature".into()));
}

#[test]
fn ui_12_filter_by_subject_author_and_hash_prefix() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    empty_commit(&git, "Add the parser");
    git.git([
        "commit",
        "--quiet",
        "--allow-empty",
        "--author=Grace Hopper <grace@example.com>",
        "-m",
        "Fix a bug",
    ])
    .unwrap();
    let grace = git.head().unwrap();
    let last = empty_commit(&git, "Update docs");
    let repo = GitRepo::open(git.path()).unwrap();

    let subjects = |text: &str| -> Vec<String> {
        let pages = all_pages(&repo, &HistoryStart::All, Some(&Filter::new(text)));
        let rows = rows(&pages);
        assert!(rows.iter().all(|row| row.graph.is_none()));
        rows.iter().map(|row| row.subject.clone()).collect()
    };
    assert_eq!(subjects("PARSER"), ["Add the parser"]);
    assert_eq!(subjects("hopper"), ["Fix a bug"]);
    assert_eq!(subjects("grace@example"), ["Fix a bug"]);
    assert_eq!(subjects(&grace[..8]), ["Fix a bug"]);
    assert_eq!(subjects(&last[..6].to_uppercase()), ["Update docs"]);
    assert!(subjects("nothing like this").is_empty());
}

#[test]
fn stash_start_is_one_row() {
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    git.commit_file("a.txt", "a\n", "A").unwrap();
    git.write("a.txt", "first change\n").unwrap();
    git.stash("first").unwrap();
    git.write("a.txt", "second change\n").unwrap();
    git.stash("second").unwrap();
    let repo = GitRepo::open(git.path()).unwrap();

    let page = repo.history(HistoryStart::Stash(1), None, None).unwrap();
    assert_eq!(page.rows.len(), 1);
    assert!(page.next.is_none());
    assert_eq!(page.rows[0].hash, git.rev_parse("stash@{1}").unwrap());
    assert!(page.rows[0].subject.ends_with("first"));
    assert!(matches!(
        repo.history(HistoryStart::Stash(2), None, None),
        Err(HistoryError::NoStash { index: 2 })
    ));
}

#[test]
fn a_cursor_from_another_start_is_refused() {
    let (_dir, repo) = generated(1_200);
    let page = repo.history(HistoryStart::All, None, None).unwrap();
    let other = repo.history(HistoryStart::Ref("main".into()), page.next, None);
    assert!(matches!(other, Err(HistoryError::CursorMismatch)));
    assert!(matches!(
        repo.history(HistoryStart::Ref("no-such-branch".into()), None, None),
        Err(HistoryError::UnknownRef { .. })
    ));
}

#[test]
fn sec_3_invalid_utf8_is_decoded_lossily() {
    // git commit would transcode the bytes, so the commit is written raw.
    let fixture = Fixture::new().unwrap();
    let git = fixture.repo("app").unwrap();
    let tree = git.git(["mktree"]).unwrap(); // the empty tree
    let mut commit = format!("tree {}\n", tree.trim()).into_bytes();
    commit.extend_from_slice(b"author Ad\xffa <ada@example.com> 1704067200 +0000\n");
    commit.extend_from_slice(b"committer Ada <ada@example.com> 1704067200 +0000\n\n");
    commit.extend_from_slice(b"Fix \xff bytes\n");
    git.write("raw-commit", commit).unwrap();
    let id = git
        .git(["hash-object", "-t", "commit", "-w", "raw-commit"])
        .unwrap();
    git.git(["update-ref", "refs/heads/main", id.trim()])
        .unwrap();

    let repo = GitRepo::open(git.path()).unwrap();
    let page = repo.history(HistoryStart::All, None, None).unwrap();
    assert_eq!(page.rows[0].subject, "Fix \u{fffd} bytes");
    assert_eq!(page.rows[0].author_name, "Ad\u{fffd}a");
}

#[test]
fn plat_2_history_of_a_repo_at_a_path_that_is_not_ascii() {
    let fixture = Fixture::new().unwrap();
    let unicode = std::ffi::OsString::from("répo-ü-日本");
    // Linux file names are bytes; macOS and Windows require Unicode names.
    #[cfg(target_os = "linux")]
    let names = [
        unicode,
        std::os::unix::ffi::OsStringExt::from_vec(b"repo-\xff".to_vec()),
    ];
    #[cfg(not(target_os = "linux"))]
    let names = [unicode];
    for name in names {
        let git = Repo::init(fixture.path().join(&name)).unwrap();
        git.commit_file("a.txt", "a\n", "First").unwrap();
        let repo = GitRepo::open(git.path()).unwrap();
        let page = repo.history(HistoryStart::All, None, None).unwrap();
        assert_eq!(page.rows.len(), 1, "{name:?}");
    }
}
