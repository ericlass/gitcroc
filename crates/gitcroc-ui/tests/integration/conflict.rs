//! M6-19a1: buffers are constructed from core chunks.
use gitcroc_core::{ConflictChunk, ConflictLabels};
use gitcroc_ui::conflict::{Buffer, Take};
fn labels() -> ConflictLabels {
    ConflictLabels {
        ours: "main\r\nbranch".into(),
        theirs: "<feature>".into(),
    }
}
fn chunk(ours: &str, theirs: &str) -> ConflictChunk {
    ConflictChunk::Hunk {
        ours: ours.into(),
        base: "base\r\n".into(),
        theirs: theirs.into(),
    }
}
fn buffer(prefix: &str, ours: &str, suffix: &str) -> Buffer {
    Buffer::build(
        &[
            ConflictChunk::Resolved {
                text: prefix.into(),
            },
            chunk(ours, "他"),
            ConflictChunk::Resolved {
                text: suffix.into(),
            },
        ],
        &labels(),
    )
}
#[test]
fn ops_8_buffer_uses_core_chunks_and_real_labels() {
    let b = buffer("<<<<<<< misleading\r\n🦀\r", "ours", "tail");
    assert_eq!(b.hunks.len(), 1);
    assert!(b.text.starts_with(
        "<<<<<<< misleading\n🦀\n<<<<<<< main branch\nours\n=======\n他\n>>>>>>> <feature>\n"
    ));
    assert!(b.text.ends_with("tail"));
    let h = &b.hunks[0];
    assert_eq!(&b.text[h.range.clone()], h.original);
    assert_eq!(h.base, "base\n");
    assert_eq!(h.taken(Take::Both), "ours\n他");
    assert_eq!(buffer("", "", "").hunks[0].taken(Take::Both), "他");
}
