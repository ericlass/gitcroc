//! Workspace-wide policy checks: crate dependency rules (ARCH-5) and lint
//! configuration (QA-4).

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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

/// The workspace's metadata, read once: `cargo metadata` takes seconds on
/// Windows, and the tests that need it run at the same time.
fn metadata() -> &'static Metadata {
    static METADATA: OnceLock<Metadata> = OnceLock::new();
    METADATA.get_or_init(|| {
        MetadataCommand::new()
            .manifest_path(workspace_root().join("Cargo.toml"))
            .exec()
            .expect("cargo metadata runs")
    })
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
    let names = names_by_id(meta);
    for crate_name in CORE_CRATES {
        let pkg = meta
            .workspace_packages()
            .into_iter()
            .find(|p| p.name.as_str() == crate_name)
            .unwrap_or_else(|| panic!("{crate_name} is a workspace member"));
        let offenders: BTreeSet<&str> = transitive_deps(meta, &pkg.id)
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
    let names = names_by_id(meta);
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
            // A crate's tests may turn on its own features, such as
            // gitcroc-ai's `mock`; that is no edge between crates.
            let own_features_for_tests = edge.0 == edge.1 && dev_only;
            if !allowed.contains(&edge) && !tool_for_tests && !own_features_for_tests {
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

fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&read_text(path))
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// `text` as numbers split at dots, when every part is one.
fn version_numbers(text: &str) -> Option<Vec<u64>> {
    text.split('.').map(|part| part.parse().ok()).collect()
}

/// D1: Rust on a stable toolchain, pinned to a release so every machine
/// builds with the same compiler (section 4 "Stack").
#[test]
fn d1_toolchain_is_a_pinned_stable_release() {
    let root = workspace_root();
    let toolchain = read_toml(&root.join("rust-toolchain.toml"));
    let channel = lookup(&toolchain, &["toolchain", "channel"])
        .and_then(|v| v.as_str())
        .expect("rust-toolchain.toml sets toolchain.channel");
    let pinned = version_numbers(channel)
        .filter(|numbers| numbers.len() == 3)
        .unwrap_or_else(|| panic!("D1: channel {channel:?} is not a release such as 1.99.0"));

    let manifest = read_toml(&root.join("Cargo.toml"));
    let package = |key: &str| {
        lookup(&manifest, &["workspace", "package", key])
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("workspace.package.{key}"))
    };
    assert_eq!(package("edition"), "2024");
    let rust_version = package("rust-version");
    let mut needed = version_numbers(rust_version)
        .unwrap_or_else(|| panic!("rust-version {rust_version:?} is not a version"));
    needed.resize(3, 0);
    assert!(
        needed <= pinned,
        "D1: rust-version {rust_version} is newer than the pinned toolchain {channel}"
    );
}

/// D2: a Tauri 2 shell, and a SolidJS front end in strict TypeScript with no
/// other UI framework.
#[test]
fn d2_shell_is_tauri_2_and_ui_is_solid_and_strict_typescript() {
    let root = workspace_root();
    let manifest = read_toml(&root.join("Cargo.toml"));
    let tauri = lookup(
        &manifest,
        &["workspace", "dependencies", "tauri", "version"],
    )
    .and_then(|v| v.as_str())
    .expect("the workspace pins tauri");
    assert_eq!(tauri.split('.').next(), Some("2"), "D2: tauri {tauri}");
    let resolved: Vec<u64> = metadata()
        .packages
        .iter()
        .filter(|p| p.name.as_str() == "tauri")
        .map(|p| p.version.major)
        .collect();
    assert_eq!(resolved, [2], "D2: one tauri, at major version 2");

    let package = read_json(&root.join("ui/package.json"));
    let mut names: Vec<&str> = Vec::new();
    for section in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(deps) = package.get(section).and_then(|v| v.as_object()) {
            names.extend(deps.keys().map(String::as_str));
        }
    }
    for needed in ["solid-js", "typescript"] {
        assert!(
            names.contains(&needed),
            "D2: ui/package.json lacks {needed}"
        );
    }
    let others = [
        "react",
        "react-dom",
        "vue",
        "svelte",
        "preact",
        "@angular/core",
    ];
    let found: Vec<&&str> = names
        .iter()
        .filter(|name| others.contains(name) || name.starts_with("@angular/"))
        .collect();
    assert!(found.is_empty(), "D2: other UI frameworks: {found:?}");

    let tsconfig = read_json(&root.join("ui/tsconfig.json"));
    assert_eq!(
        tsconfig.pointer("/compilerOptions/strict"),
        Some(&serde_json::Value::Bool(true)),
        "D2: ui/tsconfig.json is not strict"
    );
}

/// D3, the half the ARCH-1 and ARCH-2 tests leave: `gix` reads for
/// `gitcroc-git` alone, and is built without a network client, so every
/// network operation runs the git CLI.
#[test]
fn d3_gix_is_in_gitcroc_git_only_and_has_no_network_client() {
    let meta = metadata();
    let using: BTreeSet<&str> = meta
        .workspace_packages()
        .into_iter()
        .filter(|p| p.dependencies.iter().any(|dep| dep.name == "gix"))
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(using, BTreeSet::from(["gitcroc-git"]));

    let gix: Vec<&PackageId> = meta
        .packages
        .iter()
        .filter(|p| p.name.as_str() == "gix")
        .map(|p| &p.id)
        .collect();
    assert_eq!(gix.len(), 1, "one gix in the build");
    let resolve = meta.resolve.as_ref().expect("metadata includes resolve");
    let node = resolve
        .nodes
        .iter()
        .find(|node| node.id == *gix[0])
        .expect("gix has a resolve node");
    let network: Vec<&str> = node
        .features
        .iter()
        .map(|feature| feature.as_str())
        .filter(|feature| feature.contains("network") || feature.contains("http"))
        .collect();
    assert!(network.is_empty(), "D3: gix is built with {network:?}");
}

/// Crates that open HTTP connections, or the sockets under them.
const HTTP_CLIENTS: [&str; 8] = [
    "reqwest",
    "hyper",
    "ureq",
    "isahc",
    "curl",
    "attohttpc",
    "surf",
    "minreq",
];

/// The targets the app ships for (section 11), with the `cfg` values a
/// dependency's platform is matched against.
const DESKTOP_TARGETS: [(&str, &[&str]); 3] = [
    (
        "x86_64-unknown-linux-gnu",
        &[
            "unix",
            "target_os=\"linux\"",
            "target_family=\"unix\"",
            "target_arch=\"x86_64\"",
            "target_vendor=\"unknown\"",
            "target_env=\"gnu\"",
            "target_pointer_width=\"64\"",
        ],
    ),
    (
        "x86_64-pc-windows-msvc",
        &[
            "windows",
            "target_os=\"windows\"",
            "target_family=\"windows\"",
            "target_arch=\"x86_64\"",
            "target_vendor=\"pc\"",
            "target_env=\"msvc\"",
            "target_pointer_width=\"64\"",
        ],
    ),
    (
        "aarch64-apple-darwin",
        &[
            "unix",
            "target_os=\"macos\"",
            "target_family=\"unix\"",
            "target_arch=\"aarch64\"",
            "target_vendor=\"apple\"",
            "target_env=\"\"",
            "target_pointer_width=\"64\"",
        ],
    ),
];

/// Whether `dep` is a normal dependency on one of the desktop targets. Some
/// crates, such as `tauri`, depend on an HTTP client only on Android and
/// iOS.
fn normal_on_desktop(dep: &cargo_metadata::NodeDep) -> bool {
    use cargo_metadata::cargo_platform::Cfg;
    dep.dep_kinds.iter().any(|kind| {
        kind.kind == DependencyKind::Normal
            && kind.target.as_ref().is_none_or(|platform| {
                DESKTOP_TARGETS.iter().any(|(name, cfgs)| {
                    let cfgs: Vec<Cfg> = cfgs
                        .iter()
                        .map(|cfg| cfg.parse().expect("a cfg value"))
                        .collect();
                    platform.matches(name, &cfgs)
                })
            })
    })
}

/// The HTTP clients among `reached`.
fn http_clients(reached: BTreeSet<&str>) -> Vec<&str> {
    reached
        .into_iter()
        .filter(|name| HTTP_CLIENTS.contains(name))
        .collect()
}

/// D12: the app makes no network calls of its own except the user's AI
/// calls and the update check. Of the project's crates only `gitcroc-ai`
/// depends on an HTTP client directly, and `gitcroc-app` reaches one only
/// through `gitcroc-ai` and the updater plugin. Git's network operations run
/// the git CLI (ARCH-1), and `gix` has no network client (D3).
#[test]
fn d12_only_the_ai_and_update_code_open_connections() {
    let meta = metadata();
    let names = names_by_id(meta);
    let direct: BTreeSet<(&str, &str)> = meta
        .workspace_packages()
        .into_iter()
        .flat_map(|p| {
            p.dependencies
                .iter()
                .filter(|dep| dep.kind == DependencyKind::Normal)
                .filter(|dep| HTTP_CLIENTS.contains(&dep.name.as_str()))
                .map(move |dep| (p.name.as_str(), dep.name.as_str()))
        })
        .collect();
    assert_eq!(direct, BTreeSet::from([("gitcroc-ai", "reqwest")]));

    let resolve = meta.resolve.as_ref().expect("metadata includes resolve");
    let nodes: HashMap<_, _> = resolve.nodes.iter().map(|n| (&n.id, n)).collect();
    // The packages a crate reaches through normal dependencies, without
    // passing the crates in `through`.
    let reached = |root: &str, through: &[&str]| -> BTreeSet<&str> {
        let start = meta
            .workspace_packages()
            .into_iter()
            .find(|p| p.name.as_str() == root)
            .unwrap_or_else(|| panic!("{root} is a workspace member"));
        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([&start.id]);
        while let Some(id) = queue.pop_front() {
            for dep in &nodes[id].deps {
                if normal_on_desktop(dep)
                    && !through.contains(&names[&dep.pkg])
                    && seen.insert(&dep.pkg)
                {
                    queue.push_back(&dep.pkg);
                }
            }
        }
        seen.into_iter().map(|id| names[id]).collect()
    };
    let app = reached("gitcroc-app", &[]);
    assert!(app.contains("tauri-plugin-updater"), "D12: no updater");
    assert_eq!(
        http_clients(reached(
            "gitcroc-app",
            &["gitcroc-ai", "tauri-plugin-updater"]
        )),
        Vec::<&str>::new(),
        "D12: gitcroc-app reaches an HTTP client past gitcroc-ai and the updater"
    );
    for name in ["gitcroc-git", "gitcroc-core"] {
        assert_eq!(
            http_clients(reached(name, &[])),
            Vec::<&str>::new(),
            "D12: {name} reaches an HTTP client"
        );
    }
    assert_eq!(
        http_clients(reached("gitcroc-cli", &["gitcroc-ai"])),
        Vec::<&str>::new(),
        "D12: gitcroc-cli reaches an HTTP client past gitcroc-ai"
    );
}

/// The `os` values of `job` in a workflow file: an inline list, such as
/// `os: [ubuntu-22.04, windows-latest]`, or one `os:` line per `include`
/// entry. Read from the text, since the workspace has no YAML parser.
fn job_runners(workflow: &str, job: &str) -> Vec<String> {
    let start = format!("  {job}:");
    let mut lines = workflow.lines().skip_while(|line| line.trim_end() != start);
    assert!(lines.next().is_some(), "no job {job}");
    let mut runners = Vec::new();
    for line in lines.take_while(|line| !line.starts_with("  ") || line.starts_with("   ")) {
        let item = line.trim().trim_start_matches("- ");
        let Some(value) = item.strip_prefix("os:") else {
            continue;
        };
        let value = value.trim().trim_start_matches('[').trim_end_matches(']');
        runners.extend(value.split(',').map(|os| os.trim().to_owned()));
    }
    runners
}

/// D6: CI tests and builds on Windows, macOS and Linux, the Linux floor
/// glibc 2.35 (Ubuntu 22.04), macOS from 12 on Intel and Apple silicon.
#[test]
fn d6_builds_and_tests_on_windows_macos_and_linux() {
    let root = workspace_root();
    let workflows = root.join(".github/workflows");
    let ci = read_text(&workflows.join("ci.yml"));
    let build = read_text(&workflows.join("build.yml"));
    let platforms = |runners: &[String]| -> BTreeSet<&'static str> {
        runners
            .iter()
            .filter_map(|os| {
                if os.starts_with("windows-") {
                    Some("windows")
                } else if os.starts_with("macos-") {
                    Some("macos")
                } else if os.starts_with("ubuntu-") {
                    Some("linux")
                } else {
                    None
                }
            })
            .collect()
    };
    let all = BTreeSet::from(["linux", "macos", "windows"]);
    for (file, text, job) in [
        ("ci.yml", &ci, "rust"),
        ("ci.yml", &ci, "ui"),
        ("build.yml", &build, "build"),
    ] {
        let runners = job_runners(text, job);
        assert_eq!(
            platforms(&runners),
            all,
            "D6: {file} job {job} runs on {runners:?}"
        );
        assert!(
            runners.iter().any(|os| os == "ubuntu-22.04"),
            "D6: {file} job {job} has no ubuntu-22.04 runner"
        );
    }
    assert!(
        build.contains("target: universal-apple-darwin"),
        "D6: the macOS build is not universal"
    );

    let config = read_json(&root.join("crates/gitcroc-app/tauri.conf.json"));
    assert_eq!(
        config.pointer("/bundle/macOS/minimumSystemVersion"),
        Some(&serde_json::Value::from("12.0"))
    );
}

#[test]
fn d6_runners_are_read_from_inline_lists_and_include_entries() {
    let workflow = "jobs:\n  test:\n    strategy:\n      matrix:\n        os: [ubuntu-22.04, macos-latest]\n    runs-on: ${{ matrix.os }}\n  build:\n    strategy:\n      matrix:\n        include:\n          - os: windows-latest\n            bundles: nsis\n          - platform: linux\n            os: ubuntu-22.04\n  after:\n    runs-on: ubuntu-latest\n";
    assert_eq!(
        job_runners(workflow, "test"),
        ["ubuntu-22.04", "macos-latest"]
    );
    assert_eq!(
        job_runners(workflow, "build"),
        ["windows-latest", "ubuntu-22.04"]
    );
    assert_eq!(job_runners(workflow, "after"), Vec::<String>::new());
}

/// D14: MIT, in `LICENSE`, in every crate and in the bundle; the UI states
/// no other license.
#[test]
fn d14_license_is_mit() {
    let root = workspace_root();
    let license = read_text(&root.join("LICENSE"));
    assert!(
        license.starts_with("MIT License\n"),
        "D14: LICENSE is not the MIT text"
    );
    for part in [
        "Permission is hereby granted, free of charge, to any person obtaining a copy",
        "The above copyright notice and this permission notice shall be included in all",
        "THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND",
    ] {
        assert!(license.contains(part), "D14: LICENSE lacks {part:?}");
    }

    let manifest = read_toml(&root.join("Cargo.toml"));
    assert_eq!(
        lookup(&manifest, &["workspace", "package", "license"]).and_then(|v| v.as_str()),
        Some("MIT")
    );
    for pkg in metadata().workspace_packages() {
        let crate_manifest = read_toml(pkg.manifest_path.as_std_path());
        assert_eq!(
            lookup(&crate_manifest, &["package", "license", "workspace"]).and_then(|v| v.as_bool()),
            Some(true),
            "D14: {} does not inherit the workspace license",
            pkg.name
        );
        assert_eq!(pkg.license.as_deref(), Some("MIT"), "D14: {}", pkg.name);
    }

    let package = read_json(&root.join("ui/package.json"));
    let ui = package.get("license").and_then(|v| v.as_str());
    assert!(
        matches!(ui, None | Some("MIT")),
        "D14: ui/package.json license {ui:?}"
    );
    let config = read_json(&root.join("crates/gitcroc-app/tauri.conf.json"));
    assert_eq!(
        config.pointer("/bundle/license"),
        Some(&serde_json::Value::from("MIT"))
    );
}
