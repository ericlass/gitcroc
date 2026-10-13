//! Milestone M1 has no write operation (section 13). The crates that ship in
//! the app start git only through the runner (ARCH-1), and every runner call
//! in their non-test code runs a read-only subcommand. This reads their
//! source, so a new git call cannot slip past it. M2 replaces this test.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

/// The crates in the app (section 4).
const SHIPPED: [&str; 4] = ["gitcroc-git", "gitcroc-core", "gitcroc-ai", "gitcroc-app"];

/// The one file that may start a process: the runner.
const RUNNER: &str = "gitcroc-git/src/runner.rs";

/// The runner's methods that take git's arguments first.
const RUNNER_CALLS: [&str; 3] = ["run", "run_blocking", "command"];

/// The git subcommands M1 runs, none of which changes a repo: detection
/// (M0-06), the trust check (M1-04, M1-05) and the status fallback (M1-04).
/// `diff` and `log` are read with `gix`, not git.
const READ_ONLY: [&str; 3] = ["--version", "rev-parse", "status"];

/// Writes M1 allows: `git commit-graph write --reachable` writes only the
/// commit-graph cache in the git directory, never a ref, an object, the index
/// or a working file. The app offers it and runs it only when the user
/// accepts (M1-08).
const CACHE_WRITES: [&str; 1] = ["commit-graph"];

/// A source file's non-test code, comment lines removed.
struct Source {
    path: String,
    lines: Vec<String>,
}

fn sources() -> Vec<Source> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for name in SHIPPED {
        collect(&crates.join(name).join("src"), &mut files);
    }
    files.sort();
    files
        .into_iter()
        .map(|file| {
            let text = fs::read_to_string(&file).unwrap();
            let mut lines = Vec::new();
            let mut all = text.lines().peekable();
            while let Some(line) = all.next() {
                // The unit tests are a `#[cfg(test)] mod` at the end of a file.
                if line.trim() == "#[cfg(test)]"
                    && all
                        .peek()
                        .is_some_and(|next| next.trim_start().starts_with("mod "))
                {
                    break;
                }
                let code = line.trim_start();
                lines.push(if code.starts_with("//") { "" } else { line }.to_owned());
            }
            let path = file
                .strip_prefix(&crates)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            Source { path, lines }
        })
        .collect()
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

/// The string literals in `text`, in order.
fn literals(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut literal = String::new();
        while let Some(c) = chars.next() {
            match c {
                '\\' => literal.extend(chars.next()),
                '"' => break,
                c => literal.push(c),
            }
        }
        found.push(literal);
    }
    found
}

/// Where a call of `name` with a borrowed first argument starts in `line`:
/// the index just after `name(&`.
fn call(line: &str, name: &str) -> Option<usize> {
    let pattern = format!("{name}(&");
    line.match_indices(&pattern).find_map(|(at, _)| {
        let before = line[..at].chars().next_back();
        let ident = before.is_some_and(|c| c.is_alphanumeric() || c == '_');
        (!ident && !line[..at].contains("fn ")).then_some(at + pattern.len())
    })
}

/// The text from `start` in `lines[from]` to the `]` that closes the array.
fn array(lines: &[String], from: usize, start: usize) -> String {
    let mut text = String::new();
    let mut depth = 0;
    for (index, line) in lines.iter().enumerate().skip(from) {
        let line = if index == from { &line[start..] } else { line };
        for c in line.chars() {
            text.push(c);
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return text;
                    }
                }
                _ => {}
            }
        }
    }
    panic!("unclosed array");
}

/// The name of the function around `lines[at]`, and its first line.
fn enclosing_fn(lines: &[String], at: usize) -> (String, usize) {
    (0..=at)
        .rev()
        .find_map(|index| {
            let line = &lines[index];
            let rest = &line[line.find("fn ")? + 3..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then_some((name, index))
        })
        .expect("a call outside any function")
}

/// What a runner call passes: the literals of its argument array, written
/// in the call or in a `let` of the variable it passes. Empty when the
/// arguments come from elsewhere, as in a function that wraps the runner.
fn call_arguments(lines: &[String], at: usize, start: usize) -> Vec<String> {
    let line = &lines[at];
    if line[start..].starts_with('[') {
        return literals(&array(lines, at, start));
    }
    let variable: String = line[start..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let (_, first) = enclosing_fn(lines, at);
    let binding = (first..at).rev().find(|&index| {
        let code = lines[index].trim_start();
        code.strip_prefix("let ")
            .and_then(|rest| rest.strip_prefix(variable.as_str()))
            .is_some_and(|rest| rest.starts_with([' ', ':', '=']))
    });
    let Some(binding) = binding else {
        return Vec::new();
    };
    let statement: String = lines[binding..=at].join("\n");
    literals(&statement[..statement.find(';').unwrap_or(statement.len())])
}

/// git's subcommand among its arguments: the first that is not an option,
/// or the only option, as in `--version`.
fn subcommand(arguments: &[String]) -> Option<&str> {
    arguments
        .iter()
        .find(|argument| !argument.starts_with('-'))
        .or(arguments.first())
        .map(String::as_str)
}

/// Every runner call in shipped non-test code, as (place, subcommand).
/// Calls inside functions that only pass their own arguments on are
/// followed to those functions' callers.
fn git_calls(sources: &[Source]) -> Vec<(String, String)> {
    let mut wrappers: Vec<String> = RUNNER_CALLS.iter().map(|&name| name.to_owned()).collect();
    let mut checked = 0;
    let mut calls = Vec::new();
    while checked < wrappers.len() {
        let name = wrappers[checked].clone();
        checked += 1;
        for source in sources.iter().filter(|source| source.path != RUNNER) {
            for (at, line) in source.lines.iter().enumerate() {
                let Some(start) = call(line, &name) else {
                    continue;
                };
                let place = format!("{}:{}", source.path, at + 1);
                let arguments = call_arguments(&source.lines, at, start);
                match subcommand(&arguments) {
                    Some(subcommand) => calls.push((place, subcommand.to_owned())),
                    None => {
                        let (wrapper, _) = enclosing_fn(&source.lines, at);
                        assert!(
                            !RUNNER_CALLS.contains(&wrapper.as_str()),
                            "{place}: cannot tell what git runs"
                        );
                        if !wrappers.contains(&wrapper) {
                            wrappers.push(wrapper);
                        }
                    }
                }
            }
        }
    }
    calls
}

#[test]
fn m1_no_write_operation_exists_yet() {
    let sources = sources();
    assert!(
        sources.iter().any(|source| source.path == RUNNER),
        "the runner is not at {RUNNER}"
    );

    // ARCH-1: no process is started outside the runner.
    for source in &sources {
        if source.path == RUNNER {
            continue;
        }
        for (at, line) in source.lines.iter().enumerate() {
            assert!(
                !line.contains("Command::new"),
                "{}:{}: starts a process outside the git runner",
                source.path,
                at + 1
            );
        }
    }

    let calls = git_calls(&sources);
    let ran: Vec<&str> = calls.iter().map(|(_, sub)| sub.as_str()).collect();
    for subcommand in READ_ONLY.iter().chain(&CACHE_WRITES) {
        assert!(
            ran.contains(subcommand),
            "no call runs `{subcommand}`, so the scan missed it; calls: {calls:?}"
        );
    }
    for (place, subcommand) in &calls {
        assert!(
            READ_ONLY.contains(&subcommand.as_str()) || CACHE_WRITES.contains(&subcommand.as_str()),
            "{place}: runs `git {subcommand}`, which is not a read-only subcommand of M1"
        );
    }
}

#[test]
fn m1_the_scan_reads_wrapped_and_inline_arguments() {
    let lines = |text: &str| text.lines().map(str::to_owned).collect::<Vec<_>>();
    let source = |path: &str, text| Source {
        path: path.into(),
        lines: lines(text),
    };
    let sources = [
        source(
            "a.rs",
            "fn wrap(&self, args: &[&str]) {\n    let args: Vec<OsString> = args.iter().collect();\n    git.run_blocking(&args, None)\n}\nfn status() {\n    self.wrap(&[\n        \"--no-optional-locks\",\n        \"status\",\n    ]);\n}",
        ),
        source(
            "b.rs",
            "fn write() {\n    progress(format!(\"label\"));\n    let args = [\"commit\", \"-m\"].map(Into::into);\n    git.run(&args, None).await\n}",
        ),
    ];
    let calls = git_calls(&sources);
    assert_eq!(
        calls,
        vec![
            ("b.rs:4".to_owned(), "commit".to_owned()),
            ("a.rs:6".to_owned(), "status".to_owned()),
        ]
    );
}
