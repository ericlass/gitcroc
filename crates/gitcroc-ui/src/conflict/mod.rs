//! Core-chunk conflict buffer and undo recovery model (OPS-3, OPS-8).
mod buffer;
pub mod recovery;
pub use buffer::{Buffer, Hunk, Take};
