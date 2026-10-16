//! Whether a conflicted file may go to AI and, if so, exactly what is sent
//! for it (AI-4, AI-5), read from repos built in code (QA-2). Conflicts are
//! made with `git` as a terminal user would.

use std::path::PathBuf;

use gitcroc_core::{
    AiContextError, AiContextRead, AiEligibility, AiFileContext, AiOperation, AiSettings,
    ChoiceKind, GitRepo, GitRunner, Ineligible, ai_file_context,
};
use repo_gen::{Fixture, Repo};

/// git with no user or system config, so no setting of the machine takes
/// part.
fn git(fixture: &Fixture) -> GitRunner {
    let home = fixture.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    GitRunner::new(PathBuf::from("git")).with_env([
        ("HOME", home.clone().into_os_string()),
        ("XDG_CONFIG_HOME", home.into_os_string()),
        ("GIT_CONFIG_NOSYSTEM", "1".into()),
    ])
}

fn default_exclude() -> Vec<String> {
    AiSettings::default().exclude
}

/// What is read for `path` of `repo` with `exclude`, asking for `hunks`.
fn read_with(
    fixture: &Fixture,
    repo: &Repo,
    path: &str,
    exclude: &[String],
    hunks: Option<&[u32]>,
) -> Result<AiContextRead, AiContextError> {
    let open = GitRepo::open(repo.path()).unwrap();
    Ok(
        ai_file_context(&open, &git(fixture), path.as_bytes(), exclude, hunks)?
            .expect("in conflict"),
    )
}

fn read(fixture: &Fixture, repo: &Repo, path: &str) -> AiContextRead {
    read_with(fixture, repo, path, &default_exclude(), None).unwrap()
}

fn context(fixture: &Fixture, repo: &Repo, path: &str) -> AiFileContext {
    match read(fixture, repo, path) {
        AiContextRead::Context { context } => *context,
        other => panic!("{path} is not eligible: {other:?}"),
    }
}

/// Why `path` is not eligible, and the reason as the editor shows it.
fn ineligible(
    fixture: &Fixture,
    repo: &Repo,
    path: &str,
    exclude: &[String],
) -> (Ineligible, String) {
    match read_with(fixture, repo, path, exclude, None).unwrap() {
        AiContextRead::Ineligible { reason, message } => (reason, message),
        other => panic!("{path} is eligible: {other:?}"),
    }
}

/// Runs git expecting it to stop with a conflict.
fn stops(repo: &Repo, args: &[&str]) {
    let output = repo.git_output(args).unwrap();
    assert!(!output.status.success(), "git {args:?} did not stop");
    assert!(
        !repo.git(["ls-files", "--unmerged"]).unwrap().is_empty(),
        "git {args:?} left no conflict: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `main` and `feature` each changed `a.txt` once since "Base"; `main`
/// checked out.
fn diverged(fixture: &Fixture) -> Repo {
    let repo = fixture.repo("app").unwrap();
    repo.commit_file("a.txt", "a\n", "Base").unwrap();
    repo.branch("feature").unwrap();
    repo.commit_file("a.txt", "main\n", "Main").unwrap();
    repo.switch("feature").unwrap();
    repo.commit_file("a.txt", "feature\n", "Feature").unwrap();
    repo.switch("main").unwrap();
    repo
}

/// Lines `L1` to `L<count>`, with `changes` replacing some of them.
fn lines(count: usize, changes: &[(usize, &str)]) -> String {
    (1..=count)
        .map(|line| {
            changes
                .iter()
                .find(|(at, _)| *at == line)
                .map_or_else(|| format!("L{line}\n"), |(_, text)| format!("{text}\n"))
        })
        .collect()
}

/// `L<from>` to `L<to>`, with `changes` replacing some of them.
fn range(from: usize, to: usize, changes: &[(usize, &str)]) -> String {
    lines(to, changes)
        .lines()
        .skip(from - 1)
        .map(|line| format!("{line}\n"))
        .collect()
}

#[test]
fn ai_4_eligible_text_conflict() {
    let fixture = Fixture::new().unwrap();
    let repo = diverged(&fixture);
    repo.merge_with_conflicts("feature").unwrap();

    let read = context(&fixture, &repo, "a.txt");
    assert_eq!(read.path, "a.txt");
    assert_eq!(read.hunks.len(), 1);
    let open = GitRepo::open(repo.path()).unwrap();
    let conflict = open.conflict(b"a.txt").unwrap().unwrap();
    assert_eq!(
        gitcroc_core::ai_eligibility(b"a.txt", &conflict.content, &default_exclude()),
        AiEligibility::Eligible
    );
}

#[test]
fn ai_4_binary_and_delete_conflicts_are_ineligible_with_the_reason() {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    repo.write("image.bin", b"base\0data").unwrap();
    repo.commit_file("gone.txt", "kept\n", "Base").unwrap();
    repo.branch("feature").unwrap();
    repo.write("image.bin", b"main\0data").unwrap();
    repo.commit_file("gone.txt", "main\n", "Main").unwrap();
    repo.switch("feature").unwrap();
    repo.write("image.bin", b"feature\0data").unwrap();
    repo.remove("gone.txt").unwrap();
    repo.commit_all("Feature").unwrap();
    repo.switch("main").unwrap();
    repo.merge_with_conflicts("feature").unwrap();

    let exclude = default_exclude();
    assert_eq!(
        ineligible(&fixture, &repo, "image.bin", &exclude),
        (
            Ineligible::NotText {
                choice: ChoiceKind::Binary
            },
            "not a text conflict (binary)".to_owned()
        )
    );
    assert_eq!(
        ineligible(&fixture, &repo, "gone.txt", &exclude),
        (
            Ineligible::NotText {
                choice: ChoiceKind::DeletedByThem
            },
            "not a text conflict (deleted by them)".to_owned()
        )
    );
}

#[test]
fn ai_4_over_1_mb_is_ineligible() {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    // Lines of 100 bytes; one side just over 1 MB.
    let line = |text: &str| format!("{text:<99}\n");
    let big = |first: &str, count: usize| -> String {
        std::iter::once(line(first))
            .chain((1..count).map(|index| line(&index.to_string())))
            .collect()
    };
    repo.commit_file("big.txt", big("base", 100), "Base")
        .unwrap();
    repo.branch("feature").unwrap();
    let main = big("main", 10_486);
    assert!(main.len() > 1024 * 1024);
    repo.commit_file("big.txt", &main, "Main").unwrap();
    repo.switch("feature").unwrap();
    repo.commit_file("big.txt", big("feature", 100), "Feature")
        .unwrap();
    repo.switch("main").unwrap();
    repo.merge_with_conflicts("feature").unwrap();

    assert_eq!(
        ineligible(&fixture, &repo, "big.txt", &default_exclude()),
        (Ineligible::OverOneMb, "over 1 MB".to_owned())
    );
}

#[test]
fn ai_4_default_exclude_list() {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    // One file per default pattern, one in a folder, and one that matches
    // none.
    let files = [
        (".env.local", ".env*"),
        ("certs/server.pem", "*.pem"),
        ("tls.key", "*.key"),
        ("store.p12", "*.p12"),
        ("ssh/id_rsa.pub", "id_rsa*"),
    ];
    let all = || files.iter().map(|(path, _)| *path).chain(["notes.txt"]);
    for path in all() {
        repo.write(path, "base\n").unwrap();
    }
    repo.commit_all("Base").unwrap();
    repo.branch("feature").unwrap();
    for path in all() {
        repo.write(path, "main\n").unwrap();
    }
    repo.commit_all("Main").unwrap();
    repo.switch("feature").unwrap();
    for path in all() {
        repo.write(path, "feature\n").unwrap();
    }
    repo.commit_all("Feature").unwrap();
    repo.switch("main").unwrap();
    repo.merge_with_conflicts("feature").unwrap();

    let exclude = default_exclude();
    assert_eq!(exclude, [".env*", "*.pem", "*.key", "*.p12", "id_rsa*"]);
    for (path, pattern) in files {
        assert_eq!(
            ineligible(&fixture, &repo, path, &exclude),
            (
                Ineligible::Excluded {
                    pattern: pattern.to_owned()
                },
                format!("matches the exclude list ({pattern})")
            ),
            "{path}"
        );
    }
    assert!(matches!(
        read(&fixture, &repo, "notes.txt"),
        AiContextRead::Context { .. }
    ));

    // An edited list: matched against the path from the repo top too.
    let edited = vec!["certs/*".to_owned(), "NOTES.*".to_owned()];
    assert_eq!(
        ineligible(&fixture, &repo, "certs/server.pem", &edited).0,
        Ineligible::Excluded {
            pattern: "certs/*".to_owned()
        }
    );
    assert!(matches!(
        read_with(&fixture, &repo, ".env.local", &edited, None).unwrap(),
        AiContextRead::Context { .. }
    ));
    // Case is ignored where the file system ignores it.
    let notes = read_with(&fixture, &repo, "notes.txt", &edited, None).unwrap();
    if cfg!(any(windows, target_os = "macos")) {
        assert!(
            matches!(notes, AiContextRead::Ineligible { .. }),
            "{notes:?}"
        );
    } else {
        assert!(matches!(notes, AiContextRead::Context { .. }), "{notes:?}");
    }
}

/// `a.txt` of 100 lines whose lines 10, 30 and 95 both sides changed.
fn three_hunks(fixture: &Fixture) -> Repo {
    let repo = fixture.repo("app").unwrap();
    repo.commit_file("a.txt", lines(100, &[]), "Base").unwrap();
    repo.branch("feature").unwrap();
    let main = lines(100, &[(10, "main 10"), (30, "main 30"), (95, "main 95")]);
    repo.commit_file("a.txt", main, "Main").unwrap();
    repo.switch("feature").unwrap();
    let feature = lines(
        100,
        &[(10, "feature 10"), (30, "feature 30"), (95, "feature 95")],
    );
    repo.commit_file("a.txt", feature, "Feature").unwrap();
    repo.switch("main").unwrap();
    repo.merge_with_conflicts("feature").unwrap();
    repo
}

#[test]
fn ai_5_context_has_40_lines_each_side_cut_at_file_ends() {
    let fixture = Fixture::new().unwrap();
    let repo = three_hunks(&fixture);
    let read = context(&fixture, &repo, "a.txt");
    // The text around a hunk is the merged text: resolved lines and the
    // ours side of the other hunks.
    let ours = [(10, "main 10"), (30, "main 30"), (95, "main 95")];

    let hunks: Vec<_> = read.hunks.iter().map(|hunk| hunk.id).collect();
    assert_eq!(hunks, [1, 2, 3]);
    let [first, second, third] = &read.hunks[..] else {
        panic!("three hunks");
    };
    assert_eq!(
        (
            first.ours.as_str(),
            first.base.as_str(),
            first.theirs.as_str()
        ),
        ("main 10\n", "L10\n", "feature 10\n")
    );
    // Cut at the file's start: 9 lines.
    assert_eq!(first.before, range(1, 9, &ours));
    assert_eq!(first.after, range(11, 50, &ours));
    assert_eq!(first.after.lines().count(), 40);
    assert_eq!(second.before, range(1, 29, &ours));
    assert_eq!(second.after, range(31, 70, &ours));
    assert_eq!(third.before, range(55, 94, &ours));
    assert_eq!(third.before.lines().count(), 40);
    // Cut at the file's end: 5 lines.
    assert_eq!(third.after, range(96, 100, &ours));
}

#[test]
fn ai_5_only_the_requested_hunks() {
    let fixture = Fixture::new().unwrap();
    let repo = three_hunks(&fixture);
    let all = context(&fixture, &repo, "a.txt");
    let AiContextRead::Context { context: some } =
        read_with(&fixture, &repo, "a.txt", &default_exclude(), Some(&[1, 3])).unwrap()
    else {
        panic!("not eligible");
    };
    // Hunk 2 was resolved in the editor: it is not sent. Hunk 3 keeps its
    // number.
    let ids: Vec<_> = some.hunks.iter().map(|hunk| hunk.id).collect();
    assert_eq!(ids, [1, 3]);
    assert_eq!(some.hunks[0], all.hunks[0]);
    assert_eq!(some.hunks[1], all.hunks[2]);
    assert!(
        !serde_json::to_string(&*some)
            .unwrap()
            .contains("feature 30"),
        "hunk 2's theirs side was sent"
    );

    let unknown = read_with(&fixture, &repo, "a.txt", &default_exclude(), Some(&[4]));
    assert!(
        matches!(unknown, Err(AiContextError::UnknownHunk(4))),
        "{unknown:?}"
    );
}

#[test]
fn ai_5_commit_messages_per_side_since_the_merge_base() {
    let fixture = Fixture::new().unwrap();
    let repo = fixture.repo("app").unwrap();
    repo.commit_file("a.txt", "a\n", "Before the base").unwrap();
    repo.commit_file("a.txt", "base\n", "Base").unwrap();
    repo.branch("feature").unwrap();
    for index in 1..=12 {
        repo.commit_file("a.txt", format!("main {index}\n"), &format!("Main {index}"))
            .unwrap();
    }
    // Did not touch the file.
    repo.commit_file("other.txt", "other\n", "Main other")
        .unwrap();
    repo.switch("feature").unwrap();
    repo.commit_file("a.txt", "feature 1\n", "Feature 1")
        .unwrap();
    repo.commit_file("other.txt", "feature\n", "Feature other")
        .unwrap();
    repo.commit_file("a.txt", "feature 2\n", "Feature 2")
        .unwrap();
    repo.switch("main").unwrap();
    repo.merge_with_conflicts("feature").unwrap();

    let read = context(&fixture, &repo, "a.txt");
    let newest: Vec<String> = (3..=12)
        .rev()
        .map(|index| format!("Main {index}"))
        .collect();
    assert_eq!(read.ours_commits, newest);
    assert_eq!(read.theirs_commits, ["Feature 2", "Feature 1"]);
}

#[test]
fn ai_5_messages_cut_at_2000_characters() {
    let fixture = Fixture::new().unwrap();
    let repo = diverged(&fixture);
    // 1,999 characters, then a three-byte one: the cut keeps it whole.
    let long = format!("{}€ and more after the cut", "a".repeat(1_999));
    repo.switch("feature").unwrap();
    repo.commit_file("a.txt", "feature again\n", &long).unwrap();
    // Exactly 2,000 characters stays as it is.
    let exact = format!("{}é", "b".repeat(1_999));
    repo.switch("main").unwrap();
    repo.commit_file("a.txt", "main again\n", &exact).unwrap();
    repo.merge_with_conflicts("feature").unwrap();

    let read = context(&fixture, &repo, "a.txt");
    let cut = &read.theirs_commits[0];
    assert_eq!(cut.chars().count(), 2_000);
    assert_eq!(*cut, format!("{}€", "a".repeat(1_999)));
    assert_eq!(read.ours_commits[0], exact);
    assert_eq!(read.ours_commits[1], "Main");
}

/// What the sides read for `a.txt`: the operation, the labels and the
/// messages of each side.
fn sides(
    fixture: &Fixture,
    repo: &Repo,
) -> (AiOperation, String, String, Vec<String>, Vec<String>) {
    let read = context(fixture, repo, "a.txt");
    (
        read.operation,
        read.ours_label,
        read.theirs_label,
        read.ours_commits,
        read.theirs_commits,
    )
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|text| (*text).to_owned()).collect()
}

#[test]
fn ai_5_sides_for_merge_rebase_cherry_pick_revert_and_stash_apply() {
    let fixture = Fixture::new().unwrap();

    let merge = diverged(&fixture);
    merge.merge_with_conflicts("feature").unwrap();
    assert_eq!(
        sides(&fixture, &merge),
        (
            AiOperation::Merge,
            "main".to_owned(),
            "feature".to_owned(),
            strings(&["Main"]),
            strings(&["Feature"])
        )
    );

    // A rebase builds on main and replays the commit being applied.
    let rebase_fixture = Fixture::new().unwrap();
    let rebase = diverged(&rebase_fixture);
    rebase.switch("feature").unwrap();
    let feature = rebase.rev_parse("HEAD").unwrap();
    stops(&rebase, &["rebase", "main"]);
    let (operation, ours, theirs, ours_commits, theirs_commits) = sides(&rebase_fixture, &rebase);
    assert_eq!(operation, AiOperation::Rebase);
    assert_eq!(ours, "main");
    assert_eq!(theirs, format!("{} Feature (feature)", &feature[..7]));
    assert_eq!(
        (ours_commits, theirs_commits),
        (strings(&["Main"]), strings(&["Feature"]))
    );

    let pick_fixture = Fixture::new().unwrap();
    let pick = diverged(&pick_fixture);
    stops(&pick, &["cherry-pick", "feature"]);
    let (operation, _, _, ours_commits, theirs_commits) = sides(&pick_fixture, &pick);
    assert_eq!(operation, AiOperation::CherryPick);
    assert_eq!(
        (ours_commits, theirs_commits),
        (strings(&["Main"]), strings(&["Feature"]))
    );

    // The reverted commit is the merge base: only ours has commits since.
    let revert_fixture = Fixture::new().unwrap();
    let revert = revert_fixture.repo("app").unwrap();
    revert.commit_file("a.txt", "a\n", "Base").unwrap();
    let change = revert.commit_file("a.txt", "change\n", "Change").unwrap();
    revert.commit_file("a.txt", "later\n", "Later").unwrap();
    stops(&revert, &["revert", "--no-edit", &change]);
    let (operation, _, _, ours_commits, theirs_commits) = sides(&revert_fixture, &revert);
    assert_eq!(operation, AiOperation::Revert);
    assert_eq!(
        (ours_commits, theirs_commits),
        (strings(&["Later"]), Vec::new())
    );

    // A stash apply: the stash is found by its content; only its own
    // message is its side's.
    let stash_fixture = Fixture::new().unwrap();
    let stash = stash_fixture.repo("app").unwrap();
    stash.commit_file("a.txt", "a\n", "Base").unwrap();
    stash.write("a.txt", "stashed\n").unwrap();
    stash.stash("Half done").unwrap();
    // An older stash that does not match.
    stash.commit_file("a.txt", "main\n", "Main").unwrap();
    stash.write("a.txt", "other stash\n").unwrap();
    stash.stash("Unrelated").unwrap();
    stops(&stash, &["stash", "apply", "stash@{1}"]);
    let (operation, ours, theirs, ours_commits, theirs_commits) = sides(&stash_fixture, &stash);
    assert_eq!(operation, AiOperation::StashApply);
    assert_eq!((ours.as_str(), theirs.as_str()), ("main", "stash"));
    assert_eq!(
        (ours_commits, theirs_commits),
        (strings(&["Main"]), strings(&["On main: Half done"]))
    );

    // `git am` is sent as a rebase; its side is the patch's message. With
    // no commit on that side there is no merge base: ours has the file's
    // commits back to its start.
    let am_fixture = Fixture::new().unwrap();
    let am = diverged(&am_fixture);
    let patches = am_fixture.path().join("patches");
    am.git([
        "format-patch".as_ref(),
        "--quiet".as_ref(),
        "-o".as_ref(),
        patches.as_os_str(),
        "main..feature".as_ref(),
    ])
    .unwrap();
    let patch = std::fs::read_dir(&patches)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    stops(&am, &["am", "--3way", patch.to_str().unwrap()]);
    let (operation, _, theirs, ours_commits, theirs_commits) = sides(&am_fixture, &am);
    assert_eq!(operation, AiOperation::Rebase);
    assert_eq!(theirs, "patch");
    assert_eq!(
        (ours_commits, theirs_commits),
        (strings(&["Main", "Base"]), strings(&["Feature"]))
    );
}
