//! Webview security configuration (SEC-2): the content security policy in
//! `tauri.conf.json` and the capability files.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn config() -> Value {
    read_json(&app_dir().join("tauri.conf.json"))
}

/// A policy as directive name to source list. Tauri accepts a string or an
/// object of directives (string or array values).
fn parse_csp(value: &Value) -> BTreeMap<String, Vec<String>> {
    let words =
        |text: &str| -> Vec<String> { text.split_whitespace().map(str::to_owned).collect() };
    match value {
        Value::String(policy) => policy
            .split(';')
            .filter_map(|directive| {
                let mut parts = directive.split_whitespace();
                let name = parts.next()?.to_owned();
                Some((name, parts.map(str::to_owned).collect()))
            })
            .collect(),
        Value::Object(directives) => directives
            .iter()
            .map(|(name, sources)| {
                let sources = match sources {
                    Value::String(text) => words(text),
                    Value::Array(items) => items
                        .iter()
                        .flat_map(|item| words(item.as_str().unwrap()))
                        .collect(),
                    other => panic!("CSP directive {name} has value {other}"),
                };
                (name.clone(), sources)
            })
            .collect(),
        other => panic!("CSP is neither a string nor an object: {other}"),
    }
}

/// Directives that load code, styles, fonts or frames. Each must allow only
/// bundled assets; an absent one falls back to `default-src`.
const LOCAL_ONLY: [&str; 9] = [
    "script-src",
    "script-src-elem",
    "script-src-attr",
    "style-src",
    "style-src-elem",
    "style-src-attr",
    "font-src",
    "frame-src",
    "child-src",
];

fn assert_policy_is_local(name: &str, csp: &BTreeMap<String, Vec<String>>) {
    assert_eq!(
        csp.get("default-src").map(Vec::as_slice),
        Some(["'self'".to_owned()].as_slice()),
        "SEC-2: {name} must set default-src 'self'"
    );
    for directive in LOCAL_ONLY {
        if let Some(sources) = csp.get(directive) {
            for source in sources {
                assert!(
                    source == "'self'" || source == "'none'",
                    "SEC-2: {name} {directive} allows {source}"
                );
            }
        }
    }
    for (directive, sources) in csp {
        for source in sources {
            assert!(
                source != "'unsafe-eval'" && source != "'wasm-unsafe-eval'",
                "SEC-2: {name} {directive} allows {source}"
            );
        }
    }
}

#[test]
fn sec_2_csp_forbids_remote_sources() {
    let config = config();
    let security = &config["app"]["security"];

    let csp = &security["csp"];
    assert!(!csp.is_null(), "SEC-2: app.security.csp is missing");
    assert_policy_is_local("csp", &parse_csp(csp));

    // Without devCsp, `tauri dev` uses csp; with it, the same rules apply.
    if let Some(dev_csp) = security.get("devCsp").filter(|v| !v.is_null()) {
        assert_policy_is_local("devCsp", &parse_csp(dev_csp));
    }

    // Tauri must keep adding nonces and hashes for its own scripts and styles.
    let modification = &security["dangerousDisableAssetCspModification"];
    assert!(
        modification.is_null() || modification == &Value::Bool(false),
        "SEC-2: dangerousDisableAssetCspModification is set"
    );

    // The webview loads bundled assets, never a remote URL.
    let dist = config["build"]["frontendDist"].as_str().unwrap();
    assert!(
        !dist.contains("://"),
        "SEC-2: frontendDist is a URL: {dist}"
    );
}

#[test]
fn sec_2_csp_check_rejects_remote_and_eval() {
    let parse = |policy: &str| parse_csp(&Value::String(policy.to_owned()));
    let rejects = |policy: &str| {
        let csp = parse(policy);
        std::panic::catch_unwind(|| assert_policy_is_local("test", &csp)).is_err()
    };
    assert!(!rejects("default-src 'self'; frame-src 'none'"));
    assert!(rejects("default-src 'self' https://cdn.example"));
    assert!(rejects(
        "default-src 'self'; script-src 'self' https://cdn.example"
    ));
    assert!(rejects("default-src 'self'; style-src *"));
    assert!(rejects("default-src 'self'; font-src https:"));
    assert!(rejects("default-src 'self'; frame-src https://example.com"));
    assert!(rejects(
        "default-src 'self'; script-src 'self' 'unsafe-eval'"
    ));
    assert!(rejects("script-src 'self'"));
}

/// The permission identifiers granted by every capability: the files in
/// `capabilities/` plus any inline in `tauri.conf.json`.
fn granted_permissions() -> Vec<(String, String)> {
    let mut capabilities = Vec::new();
    for entry in fs::read_dir(app_dir().join("capabilities")).unwrap() {
        let path = entry.unwrap().path();
        let text = fs::read_to_string(&path).unwrap();
        let value: Value = match path.extension().and_then(|e| e.to_str()) {
            Some("json") => serde_json::from_str(&text).unwrap(),
            Some("toml") => serde_json::to_value(text.parse::<toml::Table>().unwrap()).unwrap(),
            _ => panic!("unexpected file in capabilities/: {}", path.display()),
        };
        capabilities.push((path.display().to_string(), value));
    }
    assert!(!capabilities.is_empty(), "capabilities/ is empty");
    if let Some(inline) = config()["app"]["security"]["capabilities"].as_array() {
        for capability in inline.iter().filter(|c| c.is_object()) {
            capabilities.push(("tauri.conf.json".to_owned(), capability.clone()));
        }
    }

    let mut granted = Vec::new();
    for (source, capability) in capabilities {
        let permissions = capability["permissions"].as_array().unwrap();
        for permission in permissions {
            let identifier = match permission {
                Value::String(id) => id.clone(),
                Value::Object(scoped) => scoped["identifier"].as_str().unwrap().to_owned(),
                other => panic!("{source}: permission {other}"),
            };
            granted.push((source.clone(), identifier));
        }
    }
    granted
}

#[test]
fn sec_2_capabilities_expose_no_shell_or_fs() {
    for (source, identifier) in granted_permissions() {
        for plugin in ["shell:", "fs:", "http:"] {
            assert!(
                !identifier.starts_with(plugin),
                "SEC-2: {source} grants {identifier}"
            );
        }
    }

    // Nor may the plugins be linked in at all.
    let manifest: toml::Table = fs::read_to_string(app_dir().join("Cargo.toml"))
        .unwrap()
        .parse()
        .unwrap();
    let dependencies = manifest["dependencies"].as_table().unwrap();
    for plugin in ["tauri-plugin-shell", "tauri-plugin-fs", "tauri-plugin-http"] {
        assert!(
            !dependencies.contains_key(plugin),
            "SEC-2: gitcroc-app depends on {plugin}"
        );
    }
}
