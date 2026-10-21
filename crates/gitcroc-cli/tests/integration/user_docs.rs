//! The user documentation (M5-18): every relative link in `docs/user/`,
//! `README.md` and `CONTRIBUTING.md` reaches a file that exists, and every
//! `#anchor` a heading of the file it points into, as GitHub renders them.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The Markdown files whose links are checked.
fn checked_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(root.join("docs/user"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    files.sort();
    files.push(root.join("README.md"));
    files.push(root.join("CONTRIBUTING.md"));
    files
}

/// The lines of `text` outside fenced code blocks.
fn prose_lines(text: &str) -> Vec<&str> {
    let mut fence: Option<&str> = None;
    let mut lines = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        let marker = ["```", "~~~"]
            .into_iter()
            .find(|marker| trimmed.starts_with(marker));
        match (fence, marker) {
            (None, Some(marker)) => fence = Some(marker),
            (Some(open), Some(marker)) if open == marker => fence = None,
            (None, None) => lines.push(line),
            _ => {}
        }
    }
    lines
}

/// `line` with its inline code spans blanked, so a link written as an
/// example is not taken for a link.
fn without_code_spans(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_code = false;
    for character in line.chars() {
        if character == '`' {
            in_code = !in_code;
        } else if !in_code {
            out.push(character);
        }
    }
    out
}

/// The targets of the inline links `[text](target)` and the link reference
/// definitions `[label]: target` of `line`.
fn link_targets(line: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let trimmed = line.trim_start();
    if trimmed.starts_with('[')
        && let Some((_, after)) = trimmed.split_once("]:")
        && let Some(target) = after.split_whitespace().next()
    {
        targets.push(target.trim_matches(['<', '>']).to_owned());
    }
    let mut rest = line;
    while let Some(at) = rest.find("](") {
        rest = &rest[at + 2..];
        let end = rest.find(')').unwrap_or(rest.len());
        // A title after the target: `[text](target "title")`.
        let target = rest[..end].split_whitespace().next().unwrap_or("");
        targets.push(target.trim_matches(['<', '>']).to_owned());
        rest = &rest[end..];
    }
    targets
}

fn is_external(target: &str) -> bool {
    target.contains("://") || target.starts_with("mailto:")
}

/// GitHub's anchor of a heading: lower case, punctuation dropped, spaces
/// as hyphens.
fn slug(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter(|character| character.is_alphanumeric() || matches!(character, ' ' | '-' | '_'))
        .map(|character| if character == ' ' { '-' } else { character })
        .collect()
}

/// The anchors of the headings of a Markdown file; a repeated heading gets
/// `-1`, `-2` and so on, as on GitHub.
fn anchors(text: &str) -> HashSet<String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut anchors = HashSet::new();
    for line in prose_lines(text) {
        let hashes = line
            .chars()
            .take_while(|character| *character == '#')
            .count();
        if !(1..=6).contains(&hashes) || !line[hashes..].starts_with(' ') {
            continue;
        }
        let base = slug(&line[hashes..]);
        let count = seen.entry(base.clone()).or_insert(0);
        anchors.insert(if *count == 0 {
            base.clone()
        } else {
            format!("{base}-{count}")
        });
        *count += 1;
    }
    anchors
}

/// `base` joined with the relative `target`, `..` resolved.
fn resolve(base: &Path, target: &str) -> PathBuf {
    let mut path = base.to_path_buf();
    for component in Path::new(target).components() {
        match component {
            Component::ParentDir => {
                path.pop();
            }
            Component::Normal(part) => path.push(part),
            _ => {}
        }
    }
    path
}

/// What is wrong with the links of `file`, one line per broken link.
fn broken_links(root: &Path, file: &Path) -> Vec<String> {
    let text = fs::read_to_string(file).unwrap();
    let dir = file.parent().unwrap();
    let name = file
        .strip_prefix(root)
        .unwrap_or(file)
        .display()
        .to_string();
    let mut broken = Vec::new();
    for target in prose_lines(&text)
        .into_iter()
        .flat_map(|line| link_targets(&without_code_spans(line)))
    {
        if target.is_empty() || is_external(&target) {
            continue;
        }
        let (path, anchor) = target
            .split_once('#')
            .map_or((target.as_str(), None), |(path, anchor)| {
                (path, Some(anchor))
            });
        let linked = if path.is_empty() {
            file.to_path_buf()
        } else {
            resolve(dir, path)
        };
        if !linked.exists() {
            broken.push(format!("{name}: {target} points to a missing file"));
            continue;
        }
        if let Some(anchor) = anchor {
            let headings = fs::read_to_string(&linked)
                .map(|linked_text| anchors(&linked_text))
                .unwrap_or_default();
            if !headings.contains(anchor) {
                broken.push(format!("{name}: {target} names no heading"));
            }
        }
    }
    broken
}

#[test]
fn user_docs_links_resolve() {
    let root = workspace_root();
    let files = checked_files(&root);
    assert!(
        files.len() > 2,
        "docs/user/ holds no Markdown pages; found {files:?}"
    );
    let broken: Vec<String> = files
        .iter()
        .flat_map(|file| broken_links(&root, file))
        .collect();
    assert!(broken.is_empty(), "broken links:\n{}", broken.join("\n"));
}

#[test]
fn user_docs_link_check_sees_broken_links_and_anchors() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("a.md"),
        "# Getting started\n\n## The `tree` pane\n\n## Settings\n\n## Settings\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("b.md"),
        "[ok](a.md) [ok](a.md#getting-started) [ok](a.md#the-tree-pane) [ok](a.md#settings-1)\n\
         [ok](#here) [web](https://example.com/x.md)\n\
         [gone](missing.md) [bad](a.md#nowhere)\n\
         `[code](missing.md)`\n\
         ```\n[fenced](missing.md)\n```\n\
         [ref]: missing-too.md\n\n# Here\n",
    )
    .unwrap();
    let broken = broken_links(dir.path(), &dir.path().join("b.md"));
    assert_eq!(
        broken,
        [
            "b.md: missing.md points to a missing file",
            "b.md: a.md#nowhere names no heading",
            "b.md: missing-too.md points to a missing file",
        ]
    );
}
