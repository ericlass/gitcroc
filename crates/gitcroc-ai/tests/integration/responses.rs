//! The recorded responses: real-looking replies, valid and invalid, for a
//! file whose hunks 1 and 2 were requested.

use gitcroc_ai::{HunkStatus, Problem, validate};

/// The ids every recorded response was written for.
pub const HUNK_IDS: [u32; 2] = [1, 2];

pub const VALID: &str = include_str!("../responses/valid.json");
pub const FENCED: &str = include_str!("../responses/fenced.json");
pub const PROSE_BEFORE: &str = include_str!("../responses/prose-before.json");
pub const MISSING_HUNK: &str = include_str!("../responses/missing-hunk.json");
pub const DUPLICATE_ID: &str = include_str!("../responses/duplicate-id.json");
pub const MARKER_IN_RESOLUTION: &str = include_str!("../responses/marker-in-resolution.json");
pub const UNRESOLVED_WITH_RESOLUTION: &str =
    include_str!("../responses/unresolved-with-resolution.json");

fn problems(reply: &str) -> Vec<Problem> {
    validate(reply, &HUNK_IDS).unwrap_err().0
}

#[test]
fn ai_8_recorded_valid_replies_parse() {
    for reply in [VALID, FENCED] {
        let answer = validate(reply, &HUNK_IDS).unwrap();
        assert_eq!(answer.hunks[0].status, HunkStatus::Resolved);
        assert_eq!(answer.hunks[1].status, HunkStatus::Unresolved);
        assert_eq!(answer.hunks[1].resolution, None);
    }
}

#[test]
fn ai_9_recorded_invalid_replies_name_their_rule() {
    assert!(matches!(
        problems(PROSE_BEFORE)[..],
        [Problem::NotJson { .. }]
    ));
    assert_eq!(problems(MISSING_HUNK), [Problem::Missing { id: 2 }]);
    assert!(problems(DUPLICATE_ID).contains(&Problem::Duplicate { id: 1 }));
    assert_eq!(
        problems(MARKER_IN_RESOLUTION),
        [Problem::Marker { id: 2, line: 1 }]
    );
    assert_eq!(
        problems(UNRESOLVED_WITH_RESOLUTION),
        [Problem::ResolutionWhenUnresolved { id: 2 }]
    );
}
