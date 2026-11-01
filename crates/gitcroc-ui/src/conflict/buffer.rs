//! rc.1's conflict/buffer.ts: core chunks, never parsed disk markers (OPS-8).
use gitcroc_core::{ConflictChunk, ConflictLabels};
use std::ops::Range;

#[derive(Clone, Copy)]
pub enum Take {
    Ours,
    Theirs,
    Both,
}
pub struct Hunk {
    pub id: usize,
    pub range: Range<usize>,
    pub ours: String,
    pub base: String,
    pub theirs: String,
    pub original: String,
    /// Where the marker lines start in `original`: `<<<<<<<` is at 0.
    pub head: usize,
    pub separator: usize,
    pub end: usize,
}
impl Hunk {
    pub fn taken(&self, side: Take) -> String {
        match side {
            Take::Ours => self.ours.clone(),
            Take::Theirs => self.theirs.clone(),
            Take::Both if self.ours.is_empty() => self.theirs.clone(),
            Take::Both => closed(&self.ours) + &self.theirs,
        }
    }
}
pub struct Buffer {
    pub text: String,
    pub hunks: Vec<Hunk>,
    /// The core's labels on one line each, as the headers show them (OPS-4).
    pub labels: ConflictLabels,
}
pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}
fn closed(text: &str) -> String {
    if text.is_empty() || text.ends_with('\n') {
        text.into()
    } else {
        format!("{text}\n")
    }
}
fn one_line(label: &str) -> String {
    let mut result = String::new();
    let mut newline = false;
    for c in label.chars() {
        if c == '\r' || c == '\n' {
            if !newline {
                result.push(' ');
            }
            newline = true;
        } else {
            result.push(c);
            newline = false;
        }
    }
    result
}
impl Buffer {
    /// Prepare on a background executor before constructing the editor (ARCH-7).
    pub fn build(chunks: &[ConflictChunk], labels: &ConflictLabels) -> Self {
        let mut text = String::new();
        let mut hunks = Vec::new();
        for chunk in chunks {
            match chunk {
                ConflictChunk::Resolved { text: resolved } => text.push_str(&normalize(resolved)),
                ConflictChunk::Hunk { ours, base, theirs } => {
                    let (ours, base, theirs) =
                        (normalize(ours), normalize(base), normalize(theirs));
                    let head = format!("<<<<<<< {}\n", one_line(&labels.ours));
                    let separator = head.len() + closed(&ours).len();
                    let end = separator + "=======\n".len() + closed(&theirs).len();
                    let original = format!(
                        "{head}{}=======\n{}>>>>>>> {}\n",
                        closed(&ours),
                        closed(&theirs),
                        one_line(&labels.theirs)
                    );
                    hunks.push(Hunk {
                        id: hunks.len(),
                        range: text.len()..text.len() + original.len(),
                        ours,
                        base,
                        theirs,
                        head: head.len(),
                        separator,
                        end,
                        original: original.clone(),
                    });
                    text.push_str(&original);
                }
            }
        }
        let labels = ConflictLabels {
            ours: one_line(&labels.ours),
            theirs: one_line(&labels.theirs),
        };
        Self {
            text,
            hunks,
            labels,
        }
    }
}
