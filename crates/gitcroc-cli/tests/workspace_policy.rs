//! Workspace-wide policy checks: crate dependency rules (ARCH-5) and lint
//! configuration (QA-4).

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, PackageId};

const CORE_CRATES: [&str; 3] = ["gitcroc-git", "gitcroc-core", "gitcroc-ai"];

const MEMBERS: [&str; 5] = [
    "gitcroc-git",
    "gitcroc-core",
    "gitcroc-ai",
    "gitcroc-app",
    "gitcroc-cli",
];

/// Developer tools in the workspace. They depend on no workspace crate, and
/// workspace crates may use them as dev-dependencies only (the test fixtures
/// of `repo-gen`, QA-2).
const TOOLS: [&str; 2] = ["req-coverage", "repo-gen"];

/// Internal dependency edges allowed by the Crates table in spec section 4.
const ALLOWED_EDGES: [(&str, &str); 6] = [
    ("gitcroc-core", "gitcroc-git"),
    ("gitcroc-app", "gitcroc-git"),
    ("gitcroc-app", "gitcroc-core"),
    ("gitcroc-app", "gitcroc-ai"),
    ("gitcroc-cli", "gitcroc-core"),
    ("gitcroc-cli", "gitcroc-ai"),
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn metadata() -> Metadata {
    MetadataCommand::new()
        .manifest_path(workspace_root().join("Cargo.toml"))
        .exec()
        .expect("cargo metadata runs")
}

fn names_by_id(meta: &Metadata) -> HashMap<&PackageId, &str> {
    meta.packages
        .iter()
        .map(|p| (&p.id, p.name.as_str()))
        .collect()
}

fn is_tauri_crate(name: &str) -> bool {
    name.starts_with("tauri") || name == "wry"
}

/// Every package reachable from `root` in the resolved graph, all dependency
/// kinds and all target platforms included.
fn transitive_deps<'a>(meta: &'a Metadata, root: &'a PackageId) -> HashSet<&'a PackageId> {
    let resolve = meta.resolve.as_ref().expect("metadata includes resolve");
    let nodes: HashMap<_, _> = resolve.nodes.iter().map(|n| (&n.id, n)).collect();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([root]);
    while let Some(id) = queue.pop_front() {
        let node = nodes.get(id).expect("every package id has a resolve node");
        for dep in &node.deps {
            if seen.insert(&dep.pkg) {
                queue.push_back(&dep.pkg);
            }
        }
    }
    seen
}

#[test]
fn arch_5_tauri_name_check_matches_tauri_and_wry() {
    assert!(is_tauri_crate("tauri"));
    assert!(is_tauri_crate("tauri-build"));
    assert!(is_tauri_crate("tauri-specta"));
    assert!(is_tauri_crate("wry"));
    assert!(!is_tauri_crate("gix"));
    assert!(!is_tauri_crate("thiserror"));
}

#[test]
fn arch_5_core_crates_do_not_depend_on_tauri() {
    let meta = metadata();
    let names = names_by_id(&meta);
    for crate_name in CORE_CRATES {
        let pkg = meta
            .workspace_packages()
            .into_iter()
            .find(|p| p.name.as_str() == crate_name)
            .unwrap_or_else(|| panic!("{crate_name} is a workspace member"));
        let offenders: BTreeSet<&str> = transitive_deps(&meta, &pkg.id)
            .into_iter()
            .map(|id| names[id])
            .filter(|name| is_tauri_crate(name))
            .collect();
        assert!(
            offenders.is_empty(),
            "ARCH-5: {crate_name} depends on {offenders:?}"
        );
    }
}

#[test]
fn arch_5_internal_dependencies_match_spec() {
    let meta = metadata();
    let names = names_by_id(&meta);
    let members: HashSet<&PackageId> = meta.workspace_members.iter().collect();

    let member_names: BTreeSet<&str> = members.iter().map(|id| names[id]).collect();
    let expected: BTreeSet<&str> = MEMBERS.into_iter().chain(TOOLS).collect();
    assert_eq!(member_names, expected);

    let allowed: BTreeSet<(&str, &str)> = ALLOWED_EDGES.into_iter().collect();
    let resolve = meta.resolve.as_ref().expect("metadata includes resolve");
    let mut forbidden = BTreeSet::new();
    for node in resolve.nodes.iter().filter(|n| members.contains(&n.id)) {
        for dep in node.deps.iter().filter(|d| members.contains(&d.pkg)) {
            let edge = (names[&node.id], names[&dep.pkg]);
            let dev_only = dep
                .dep_kinds
                .iter()
                .all(|kind| kind.kind == DependencyKind::Development);
            let tool_for_tests = TOOLS.contains(&edge.1) && !TOOLS.contains(&edge.0) && dev_only;
            if !allowed.contains(&edge) && !tool_for_tests {
                forbidden.insert(edge);
            }
        }
    }
    assert!(
        forbidden.is_empty(),
        "ARCH-5: internal dependencies not allowed by the Crates table: {forbidden:?}"
    );
}

fn read_toml(path: &Path) -> toml::Table {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.parse()
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn lookup<'a>(table: &'a toml::Table, path: &[&str]) -> Option<&'a toml::Value> {
    let (first, rest) = path.split_first()?;
    rest.iter()
        .try_fold(table.get(*first)?, |value, key| value.get(*key))
}

#[test]
fn qa_4_workspace_lints_forbid_unsafe_and_unwrap() {
    let root = workspace_root();
    let manifest = read_toml(&root.join("Cargo.toml"));
    let level = |path: &[&str]| lookup(&manifest, path).and_then(|v| v.as_str());
    assert_eq!(
        level(&["workspace", "lints", "rust", "unsafe_code"]),
        Some("forbid")
    );
    assert_eq!(
        level(&["workspace", "lints", "clippy", "unwrap_used"]),
        Some("deny")
    );
    assert_eq!(
        level(&["workspace", "lints", "clippy", "expect_used"]),
        Some("deny")
    );

    // unwrap and expect stay allowed in tests only.
    let clippy = read_toml(&root.join("clippy.toml"));
    assert_eq!(
        lookup(&clippy, &["allow-unwrap-in-tests"]).and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        lookup(&clippy, &["allow-expect-in-tests"]).and_then(|v| v.as_bool()),
        Some(true)
    );

    // Every crate opts in to the workspace lints.
    for pkg in metadata().workspace_packages() {
        let crate_manifest = read_toml(pkg.manifest_path.as_std_path());
        assert_eq!(
            lookup(&crate_manifest, &["lints", "workspace"]).and_then(|v| v.as_bool()),
            Some(true),
            "QA-4: {} does not set [lints] workspace = true",
            pkg.name
        );
    }
}
