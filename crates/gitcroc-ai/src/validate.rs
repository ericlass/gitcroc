//! Whether a model's reply is a valid answer for the requested hunks
//! (AI-8, AI-9).

use std::collections::BTreeMap;
use std::fmt;

use crate::answer::{FileAnswer, HunkAnswer, HunkStatus};

/// git's conflict marker length.
const MARKER_SIZE: usize = 7;

/// The characters git starts its conflict marker lines with.
const MARKERS: [u8; 4] = *b"<|=>";

/// The first line of `text`, counted from 1, that is a conflict marker
/// line: exactly seven `<`, `=`, `|` or `>` at its start, then a space or
/// the line's end. A copy of `gitcroc_git::first_marker_line` (OPS-5), since
/// this crate depends on no other (ARCH-5); a test in `gitcroc-app` checks
/// that the two agree.
pub fn first_marker_line(text: &str) -> Option<u32> {
    text.lines()
        .position(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line).as_bytes();
            line.first().is_some_and(|first| {
                MARKERS.contains(first)
                    && line.len() >= MARKER_SIZE
                    && line.iter().take(MARKER_SIZE).all(|byte| byte == first)
                    && matches!(line.get(MARKER_SIZE), None | Some(b' '))
            })
        })
        .and_then(|index| u32::try_from(index.saturating_add(1)).ok())
}

/// One rule a reply breaks, with the hunk it breaks it for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// The reply is not one JSON object of the shape in section 7.
    NotJson {
        error: String,
    },
    Missing {
        id: u32,
    },
    Duplicate {
        id: u32,
    },
    NotRequested {
        id: u32,
    },
    NoResolution {
        id: u32,
    },
    ResolutionWhenUnresolved {
        id: u32,
    },
    NoExplanation {
        id: u32,
    },
    /// `line` is counted from 1 within the resolution.
    Marker {
        id: u32,
        line: u32,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotJson { error } => {
                write!(
                    f,
                    "the reply is not one JSON object of the required shape: {error}"
                )
            }
            Self::Missing { id } => write!(f, "hunk {id} is missing"),
            Self::Duplicate { id } => write!(f, "hunk {id} is listed twice"),
            Self::NotRequested { id } => write!(f, "hunk {id} was not requested"),
            Self::NoResolution { id } => write!(f, "hunk {id} is resolved but has no resolution"),
            Self::ResolutionWhenUnresolved { id } => {
                write!(f, "hunk {id} is unresolved but has a resolution")
            }
            Self::NoExplanation { id } => write!(f, "hunk {id} has no explanation"),
            Self::Marker { id, line } => {
                write!(
                    f,
                    "the resolution of hunk {id} has a conflict marker line (line {line})"
                )
            }
        }
    }
}

/// Every rule a reply breaks, in the order they were found; never empty.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct Invalid(pub Vec<Problem>);

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, problem) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{problem}")?;
        }
        Ok(())
    }
}

/// `text` without surrounding whitespace and at most one surrounding
/// Markdown code fence, whose opening line may name a language.
fn unfenced(text: &str) -> &str {
    let text = text.trim();
    let Some(rest) = text.strip_prefix("```") else {
        return text;
    };
    let Some((info, body)) = rest.split_once('\n') else {
        return text;
    };
    if info.contains('`') {
        return text;
    }
    match body.trim_end().strip_suffix("```") {
        Some(body) if body.is_empty() || body.ends_with('\n') => body.trim(),
        _ => text,
    }
}

/// The problems of one hunk on its own.
fn hunk_problems(hunk: &HunkAnswer, problems: &mut Vec<Problem>) {
    let id = hunk.id;
    match (hunk.status, &hunk.resolution) {
        (HunkStatus::Resolved, None) => problems.push(Problem::NoResolution { id }),
        (HunkStatus::Resolved, Some(resolution)) => {
            if let Some(line) = first_marker_line(resolution) {
                problems.push(Problem::Marker { id, line });
            }
        }
        (HunkStatus::Unresolved, Some(_)) => {
            problems.push(Problem::ResolutionWhenUnresolved { id })
        }
        (HunkStatus::Unresolved, None) => {}
    }
    if hunk.explanation.trim().is_empty() {
        problems.push(Problem::NoExplanation { id });
    }
}

/// The answer in `reply` for the hunks `hunk_ids`, in that order, or every
/// rule it breaks (AI-8, AI-9).
pub fn validate(reply: &str, hunk_ids: &[u32]) -> Result<FileAnswer, Invalid> {
    let answer: FileAnswer = serde_json::from_str(unfenced(reply)).map_err(|error| {
        Invalid(vec![Problem::NotJson {
            error: error.to_string(),
        }])
    })?;

    let mut problems = Vec::new();
    let mut by_id = BTreeMap::new();
    for hunk in answer.hunks {
        // Each reported once, however often the id repeats.
        let id = hunk.id;
        if !hunk_ids.contains(&id) {
            if !problems.contains(&Problem::NotRequested { id }) {
                problems.push(Problem::NotRequested { id });
            }
            continue;
        }
        if by_id.contains_key(&id) {
            if !problems.contains(&Problem::Duplicate { id }) {
                problems.push(Problem::Duplicate { id });
            }
            continue;
        }
        hunk_problems(&hunk, &mut problems);
        by_id.insert(id, hunk);
    }

    let mut hunks = Vec::with_capacity(hunk_ids.len());
    for id in hunk_ids {
        match by_id.remove(id) {
            Some(hunk) => hunks.push(hunk),
            None => problems.push(Problem::Missing { id: *id }),
        }
    }

    if problems.is_empty() {
        Ok(FileAnswer { hunks })
    } else {
        Err(Invalid(problems))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::answer::Confidence;

    fn reply(hunks: &str) -> String {
        format!(r#"{{"hunks": [{hunks}]}}"#)
    }

    fn resolved(id: u32, resolution: &str) -> String {
        let resolution = serde_json::to_string(resolution).unwrap();
        format!(
            r#"{{"id": {id}, "status": "resolved", "resolution": {resolution}, "explanation": "Kept both.", "confidence": "high"}}"#
        )
    }

    fn problems(reply: &str, ids: &[u32]) -> Vec<Problem> {
        validate(reply, ids).unwrap_err().0
    }

    #[test]
    fn ai_8_statuses_and_confidences() {
        for (status, resolution) in [("resolved", r#""x\n""#), ("unresolved", "null")] {
            for confidence in ["high", "medium", "low"] {
                let text = reply(&format!(
                    r#"{{"id": 1, "status": "{status}", "resolution": {resolution}, "explanation": "Why.", "confidence": "{confidence}"}}"#
                ));
                let answer = validate(&text, &[1]).unwrap();
                let hunk = &answer.hunks[0];
                assert_eq!(serde_json::to_value(hunk.status).unwrap(), status);
                assert_eq!(serde_json::to_value(hunk.confidence).unwrap(), confidence);
            }
        }
        for (status, confidence) in [
            ("merged", "high"),
            ("Resolved", "high"),
            ("resolved", "certain"),
            ("resolved", "HIGH"),
            ("resolved", "0.9"),
        ] {
            let text = reply(&format!(
                r#"{{"id": 1, "status": "{status}", "resolution": "x", "explanation": "Why.", "confidence": "{confidence}"}}"#
            ));
            assert!(
                matches!(problems(&text, &[1])[..], [Problem::NotJson { .. }]),
                "{status} {confidence}"
            );
        }
        // Every field of the shape is required, and no other is allowed.
        let no_confidence =
            reply(r#"{"id": 1, "status": "resolved", "resolution": "x", "explanation": "Why."}"#);
        assert!(matches!(
            problems(&no_confidence, &[1])[..],
            [Problem::NotJson { .. }]
        ));
        let extra = reply(&resolved(1, "x").replace('}', r#", "note": "hi"}"#));
        assert!(matches!(
            problems(&extra, &[1])[..],
            [Problem::NotJson { .. }]
        ));
        let top_level = r#"{"hunks": [], "summary": "none"}"#;
        assert!(matches!(
            problems(top_level, &[])[..],
            [Problem::NotJson { .. }]
        ));
    }

    #[test]
    fn ai_8_unresolved_hunk_has_explanation_and_no_resolution() {
        let absent = reply(
            r#"{"id": 1, "status": "unresolved", "explanation": "Both rewrote it.", "confidence": "low"}"#,
        );
        let answer = validate(&absent, &[1]).unwrap();
        assert_eq!(answer.hunks[0].status, HunkStatus::Unresolved);
        assert_eq!(answer.hunks[0].resolution, None);

        let null = absent.replace(r#""explanation""#, r#""resolution": null, "explanation""#);
        assert!(validate(&null, &[1]).is_ok());

        let with_resolution =
            absent.replace(r#""explanation""#, r#""resolution": "x", "explanation""#);
        assert_eq!(
            problems(&with_resolution, &[1]),
            [Problem::ResolutionWhenUnresolved { id: 1 }]
        );
        let empty_resolution =
            absent.replace(r#""explanation""#, r#""resolution": "", "explanation""#);
        assert_eq!(
            problems(&empty_resolution, &[1]),
            [Problem::ResolutionWhenUnresolved { id: 1 }]
        );
        let no_explanation = absent.replace("Both rewrote it.", "  ");
        assert_eq!(
            problems(&no_explanation, &[1]),
            [Problem::NoExplanation { id: 1 }]
        );

        // The resolved side of the same rule.
        let no_resolution = absent.replace("unresolved", "resolved");
        assert_eq!(
            problems(&no_resolution, &[1]),
            [Problem::NoResolution { id: 1 }]
        );
        // An empty resolution removes the hunk's lines, which is an answer.
        assert_eq!(
            validate(&reply(&resolved(1, "")), &[1]).unwrap().hunks[0].resolution,
            Some(String::new())
        );
    }

    #[test]
    fn ai_9_every_requested_hunk_exactly_once() {
        let all = reply(&[resolved(2, "b\n"), resolved(1, "a\n")].join(","));
        let answer = validate(&all, &[1, 2]).unwrap();
        // In the requested order, whatever the reply's.
        assert_eq!(
            answer.hunks.iter().map(|hunk| hunk.id).collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(answer.hunks[0].confidence, Confidence::High);

        let missing = reply(&resolved(1, "a\n"));
        assert_eq!(problems(&missing, &[1, 2]), [Problem::Missing { id: 2 }]);
        let duplicate =
            reply(&[resolved(1, "a\n"), resolved(2, "b\n"), resolved(1, "a\n")].join(","));
        assert_eq!(
            problems(&duplicate, &[1, 2]),
            [Problem::Duplicate { id: 1 }]
        );
        let extra = reply(&[resolved(1, "a\n"), resolved(3, "c\n")].join(","));
        assert_eq!(problems(&extra, &[1]), [Problem::NotRequested { id: 3 }]);

        assert_eq!(
            Problem::Duplicate { id: 3 }.to_string(),
            "hunk 3 is listed twice"
        );
        let both = Invalid(vec![
            Problem::Missing { id: 2 },
            Problem::NotRequested { id: 4 },
        ]);
        assert_eq!(
            both.to_string(),
            "hunk 2 is missing; hunk 4 was not requested"
        );
    }

    #[test]
    fn ai_9_no_resolution_contains_a_marker_line() {
        for (resolution, line) in [
            ("a\n<<<<<<< HEAD\nb\n", 2),
            ("=======\n", 1),
            ("a\nb\n|||||||", 3),
            ("a\r\n>>>>>>> theirs\r\n", 2),
        ] {
            assert_eq!(
                problems(&reply(&resolved(1, resolution)), &[1]),
                [Problem::Marker { id: 1, line }],
                "{resolution:?}"
            );
        }
        for resolution in [
            "<<<<<<<<\n",
            " =======\n",
            "======\n",
            ">>>>>>>x\n",
            "// =======\n",
        ] {
            assert!(
                validate(&reply(&resolved(1, resolution)), &[1]).is_ok(),
                "{resolution:?}"
            );
        }
        assert_eq!(
            Problem::Marker { id: 2, line: 5 }.to_string(),
            "the resolution of hunk 2 has a conflict marker line (line 5)"
        );
    }

    #[test]
    fn ai_9_reply_is_one_object_after_one_fence() {
        let body = reply(&resolved(1, "a\n"));
        for text in [
            format!("  {body}\n\n"),
            format!("```json\n{body}\n```"),
            format!("```\n{body}\n```\n"),
        ] {
            assert!(validate(&text, &[1]).is_ok(), "{text}");
        }
        for text in [
            format!("Here is the JSON:\n{body}"),
            format!("{body}\nHope this helps."),
            format!("````\n```json\n{body}\n```\n````"),
            format!("```json\n```json\n{body}\n```\n```"),
            format!("```json\n{body}"),
            format!("{body}{body}"),
            "[]".to_owned(),
            String::new(),
        ] {
            assert!(
                matches!(problems(&text, &[1])[..], [Problem::NotJson { .. }]),
                "{text}"
            );
        }
    }
}
