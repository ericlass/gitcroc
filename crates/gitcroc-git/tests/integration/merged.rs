//! The merged history of several repos (PERF-5), on fixture repos with set
//! commit dates.

use std::path::PathBuf;

use gitcroc_git::{MergedPage, PAGE_SIZE, merged_history};
use repo_gen::{Fixture, Repo};

/// Commits `subject` at fixture minute `minute + 1`: `commit_file` runs
/// `add`, then `commit` a minute later.
fn commit_at(repo: &Repo, minute: u64, subject: &str) {
    repo.set_clock(minute);
    repo.commit_file("file.txt", subject, subject).unwrap();
}

fn subjects(page: &MergedPage) -> Vec<(usize, &str)> {
    page.rows
        .iter()
        .map(|row| (row.repo, row.row.subject.as_str()))
        .collect()
}

fn path(repo: &Repo) -> PathBuf {
    repo.path().to_path_buf()
}

#[test]
fn perf_5_merged_history_is_newest_first_across_repos() {
    let fixture = Fixture::new().unwrap();
    let a = fixture.repo("a").unwrap();
    let b = fixture.repo("b").unwrap();
    commit_at(&a, 0, "a1");
    commit_at(&b, 5, "b1");
    commit_at(&a, 10, "a2");
    commit_at(&b, 10, "b2");
    commit_at(&a, 20, "a3");
    commit_at(&b, 30, "b3");

    let page = merged_history(&[path(&a), path(&b)], None, None).unwrap();
    // a2 and b2 have the same date: the repo given first leads.
    assert_eq!(
        subjects(&page),
        [
            (1, "b3"),
            (0, "a3"),
            (0, "a2"),
            (1, "b2"),
            (1, "b1"),
            (0, "a1")
        ]
    );
    assert!(page.next.is_none());
    assert!(page.rows.iter().all(|row| row.row.graph.is_none()));
    let page = merged_history(&[path(&b), path(&a)], None, None).unwrap();
    assert_eq!(
        subjects(&page)[2..4],
        [(0, "b2"), (1, "a2")],
        "ties follow the order of the target set"
    );

    // A commit in two repos shows once per repo, the same commit date
    // keeping the repos in order.
    let copy = a.clone_to(fixture.path().join("copy")).unwrap();
    let page = merged_history(&[path(&a), path(&copy)], None, None).unwrap();
    assert_eq!(
        subjects(&page),
        [
            (0, "a3"),
            (1, "a3"),
            (0, "a2"),
            (1, "a2"),
            (0, "a1"),
            (1, "a1")
        ]
    );
    assert_eq!(page.rows[0].row.hash, page.rows[1].row.hash);
}

#[test]
fn perf_5_merged_history_walks_only_what_the_page_needs() {
    let fixture = Fixture::new().unwrap();
    let old = fixture.repo("old").unwrap();
    commit_at(&old, 0, "old 1");
    let messages: Vec<String> = (2..=5_000).map(|n| format!("old {n}")).collect();
    old.empty_commits(&messages).unwrap();
    let new = fixture.repo("new").unwrap();
    commit_at(&new, 100_000, "new 1");
    let messages: Vec<String> = (2..=600).map(|n| format!("new {n}")).collect();
    new.empty_commits(&messages).unwrap();
    for repo in [&old, &new] {
        repo.git(["commit-graph", "write", "--reachable"]).unwrap();
    }
    let paths = [path(&old), path(&new)];

    let first = merged_history(&paths, None, None).unwrap();
    assert_eq!(first.rows.len(), PAGE_SIZE);
    assert!(first.rows.iter().all(|row| row.repo == 1));
    let cursor = first.next.clone().unwrap();
    assert_eq!(
        cursor.rows_read(0),
        1,
        "only old's newest commit was read as a row"
    );
    assert!(
        cursor.commits_read(0) <= 2,
        "old's walk read {} commits",
        cursor.commits_read(0)
    );
    assert_eq!(cursor.rows_read(1), PAGE_SIZE as u64 + 1);

    // The next page takes new's last 100 commits, then old's newest 400.
    let second = merged_history(&paths, Some(cursor), None).unwrap();
    let rows = subjects(&second);
    assert_eq!(rows.len(), PAGE_SIZE);
    assert_eq!(rows[99], (1, "new 1"));
    assert_eq!(rows[100], (0, "old 5000"));
    assert_eq!(rows[499], (0, "old 4601"));
}

#[test]
fn perf_5_unborn_and_missing_repos_contribute_no_rows() {
    let fixture = Fixture::new().unwrap();
    let a = fixture.repo("a").unwrap();
    commit_at(&a, 0, "a1");
    let unborn = fixture.repo("unborn").unwrap();
    let missing = fixture.path().join("missing");

    let page = merged_history(&[path(&unborn), missing, path(&a)], None, None).unwrap();
    assert_eq!(subjects(&page), [(2, "a1")]);
    assert!(page.next.is_none());
}
