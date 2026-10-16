//! Every write goes through the runner and lives in `gitcroc-git::ops`
//! (ARCH-1, M2-01). The crates that ship in the app start a process only in
//! the runner, and every runner call in their non-test code outside `ops`
//! runs a read-only subcommand. This reads their source, so a new git call
//! cannot slip past it.

use std::fs;
use std::path::{Path, PathBuf};

/// The crates in the app (section 4).
const SHIPPED: [&str; 4] = ["gitcroc-git", "gitcroc-core", "gitcroc-ai", "gitcroc-app"];

/// The one file that may start a git process: the runner.
const RUNNER: &str = "gitcroc-git/src/runner.rs";

/// The one file that may start another program: the file manager or a
/// terminal, never git (M2-14).
const LAUNCHER: &str = "gitcroc-app/src/platform/launch.rs";

/// The runner's methods that take git's arguments first.
const RUNNER_CALLS: [&str; 5] = [
    "run",
    "run_blocking",
    "run_write",
    "command",
    "write_command",
];

/// The runner's methods for writes, called only in [`OPS`].
const WRITE_CALLS: [&str; 2] = ["run_write", "write_command"];

/// The home of every write.
const OPS: &str = "gitcroc-git/src/ops/";

/// The git subcommands run outside [`OPS`], none of which changes a repo:
/// detection (M0-06), the trust check (M1-04, M1-05), the status fallback
/// (M1-04), the conflict prediction of a group merge (GRP-13, M3-12) and
/// the fallback of the commits AI-5 sends (M4-07). `merge-tree` writes
/// objects to the object store only, never a ref, the index or a working
/// file, as the commit-graph file of M1 adds only what git can read
/// faster. `diff` is read with `gix`, not git.
const READ_ONLY: [&str; 5] = ["--version", "rev-parse", "status", "merge-tree", "log"];

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

/// A runner call in shipped non-test code.
#[derive(Debug, PartialEq, Eq)]
struct Call {
    place: String,
    subcommand: String,
    /// The runner method it reaches.
    method: String,
}

/// Every runner call in shipped non-test code. Calls inside functions that
/// only pass their own arguments on are followed to those functions'
/// callers.
fn git_calls(sources: &[Source]) -> Vec<Call> {
    // Each wrapper with the runner method it reaches.
    let mut wrappers: Vec<(String, String)> = RUNNER_CALLS
        .iter()
        .map(|&name| (name.to_owned(), name.to_owned()))
        .collect();
    let mut checked = 0;
    let mut calls = Vec::new();
    while checked < wrappers.len() {
        let (name, method) = wrappers[checked].clone();
        checked += 1;
        for source in sources.iter().filter(|source| source.path != RUNNER) {
            for (at, line) in source.lines.iter().enumerate() {
                let Some(start) = call(line, &name) else {
                    continue;
                };
                let place = format!("{}:{}", source.path, at + 1);
                let arguments = call_arguments(&source.lines, at, start);
                match subcommand(&arguments) {
                    Some(subcommand) => calls.push(Call {
                        place,
                        subcommand: subcommand.to_owned(),
                        method: method.clone(),
                    }),
                    None => {
                        let (wrapper, _) = enclosing_fn(&source.lines, at);
                        assert!(
                            !RUNNER_CALLS.contains(&wrapper.as_str()),
                            "{place}: cannot tell what git runs"
                        );
                        if !wrappers.iter().any(|(known, _)| *known == wrapper) {
                            wrappers.push((wrapper, method.clone()));
                        }
                    }
                }
            }
        }
    }
    calls
}

#[test]
fn arch_1_writes_go_through_the_runner() {
    let sources = sources();
    assert!(
        sources.iter().any(|source| source.path == RUNNER),
        "the runner is not at {RUNNER}"
    );

    // No process is started outside the runner, apart from the launcher's.
    assert!(
        sources.iter().any(|source| source.path == LAUNCHER),
        "the launcher is not at {LAUNCHER}"
    );
    for source in &sources {
        if source.path == RUNNER || source.path == LAUNCHER {
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
    let (writes, reads): (Vec<&Call>, Vec<&Call>) =
        calls.iter().partition(|call| call.place.starts_with(OPS));
    let ran: Vec<&str> = reads.iter().map(|call| call.subcommand.as_str()).collect();
    for subcommand in READ_ONLY {
        assert!(
            ran.contains(&subcommand),
            "no call runs `{subcommand}`, so the scan missed it; calls: {calls:?}"
        );
    }
    assert!(
        writes.iter().any(|call| call.subcommand == "commit-graph"),
        "the scan found no write in {OPS}; calls: {calls:?}"
    );
    for call in reads {
        assert!(
            READ_ONLY.contains(&call.subcommand.as_str()),
            "{}: runs `git {}` outside {OPS}, which is not a read-only subcommand",
            call.place,
            call.subcommand
        );
        assert!(
            !WRITE_CALLS.contains(&call.method.as_str()),
            "{}: runs a write outside {OPS}",
            call.place
        );
    }
    for call in writes {
        assert!(
            WRITE_CALLS.contains(&call.method.as_str()),
            "{}: a write in {OPS} must run with `run_write`, not `{}`",
            call.place,
            call.method
        );
    }
}

#[test]
fn arch_1_the_scan_reads_wrapped_and_inline_arguments() {
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
    let calls: Vec<(String, String, String)> = git_calls(&sources)
        .into_iter()
        .map(|call| (call.place, call.subcommand, call.method))
        .collect();
    let call = |place: &str, subcommand: &str, method: &str| {
        (place.to_owned(), subcommand.to_owned(), method.to_owned())
    };
    assert_eq!(
        calls,
        vec![
            call("b.rs:4", "commit", "run"),
            call("a.rs:6", "status", "run_blocking"),
        ]
    );
}
