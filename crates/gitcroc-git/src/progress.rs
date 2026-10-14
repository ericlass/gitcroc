//! git's `--progress` lines (M2-07): `Receiving objects:  45% (450/1000),
//! 1.2 MiB | 2 MiB/s`, `Enumerating objects: 5, done.`, with `remote: `
//! before the remote's own. Only the wording of phases is read; whether an
//! operation did anything is never decided from it.

/// One phase of a transfer and how far it has come: `done` of `total`. A
/// `total` of 0 means the phase gives no total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// As git names it, such as `Receiving objects`.
    pub step: String,
    pub done: u32,
    pub total: u32,
}

/// The phase `line` reports, or `None` when it is not a progress line.
pub fn parse_progress(line: &str) -> Option<Progress> {
    let line = line.strip_prefix("remote: ").unwrap_or(line).trim();
    let (step, rest) = line.split_once(": ")?;
    // Two or more words of letters, the first capitalized.
    let is_phase = step.starts_with(|first: char| first.is_ascii_uppercase())
        && step.contains(' ')
        && step.split(' ').all(|word| {
            !word.is_empty() && word.chars().all(|letter| letter.is_ascii_alphabetic())
        });
    if !is_phase {
        return None;
    }
    let rest = rest.trim_start();
    let (done, total) = if let Some((_, counts)) = rest.split_once(" (") {
        // `45% (450/1000)`
        let counts = counts.split_once(')')?.0;
        let (done, total) = counts.split_once('/')?;
        (done.parse().ok()?, total.parse().ok()?)
    } else if rest.starts_with("done") {
        (0, 0)
    } else {
        // `5, done.`, or a bare count while it runs.
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        (digits.parse().ok()?, 0)
    };
    Some(Progress {
        step: step.to_owned(),
        done,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(step: &str, done: u32, total: u32) -> Option<Progress> {
        Some(Progress {
            step: step.to_owned(),
            done,
            total,
        })
    }

    #[test]
    fn phases_with_and_without_numbers() {
        assert_eq!(
            parse_progress("Receiving objects:  45% (450/1000), 1.20 MiB | 2.00 MiB/s"),
            progress("Receiving objects", 450, 1000)
        );
        assert_eq!(
            parse_progress("remote: Counting objects: 100% (5/5), done."),
            progress("Counting objects", 5, 5)
        );
        assert_eq!(
            parse_progress("remote: Enumerating objects: 5, done."),
            progress("Enumerating objects", 5, 0)
        );
        assert_eq!(
            parse_progress("Resolving deltas:   0% (0/3)"),
            progress("Resolving deltas", 0, 3)
        );
        assert_eq!(
            parse_progress("Writing objects: 100% (3/3), 250 bytes | 250.00 KiB/s, done."),
            progress("Writing objects", 3, 3)
        );
        assert_eq!(
            parse_progress("remote: Compressing objects: 100% (2/2), done."),
            progress("Compressing objects", 2, 2)
        );
    }

    #[test]
    fn other_lines_are_not_progress() {
        for line in [
            "From file:///tmp/origin",
            " * [new branch]      main       -> origin/main",
            "fatal: 'nope' does not appear to be a git repository",
            "error: failed to push some refs to 'x'",
            "To file:///tmp/origin",
            "hint: Updates were rejected",
            "Note: switching to 'abc'.",
            "Total 3 (delta 0), reused 0 (delta 0), pack-reused 0",
            "",
        ] {
            assert_eq!(parse_progress(line), None, "{line:?}");
        }
    }
}
