//! Branch and tag names the user types, checked against git's ref format
//! rules before any command runs (SEC-4): the rules of `git
//! check-ref-format --branch`, in Rust, each failure naming its rule.
//! Tags (M2-12) use the same rules.

/// The rule a name breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RefNameError {
    #[error("the name is empty")]
    Empty,
    #[error("the name cannot start with '-'")]
    LeadingDash,
    #[error("'{0}' is not a valid name")]
    Reserved(&'static str),
    #[error("the name cannot contain spaces")]
    Space,
    #[error("the name cannot contain control characters")]
    Control,
    #[error("the name cannot contain '{0}'")]
    Forbidden(char),
    #[error("the name cannot contain '..'")]
    DoubleDot,
    #[error("the name cannot contain '@{{'")]
    AtBrace,
    #[error("the name cannot start or end with '/' or contain '//'")]
    EmptyPart,
    #[error("no part of the name between slashes can start with '.'")]
    DotPart,
    #[error("no part of the name between slashes can end with '.lock'")]
    LockPart,
    #[error("the name cannot end with '.'")]
    TrailingDot,
}

/// Characters no ref name may contain, besides spaces and control
/// characters.
const FORBIDDEN: [char; 7] = ['~', '^', ':', '?', '*', '[', '\\'];

/// Checks `name` as a branch name, as `git check-ref-format --branch` does,
/// except that `@` and `-`, which git reads as `HEAD` and the previous
/// branch, are refused.
pub fn check_branch_name(name: &str) -> Result<(), RefNameError> {
    if name.is_empty() {
        return Err(RefNameError::Empty);
    }
    if name.starts_with('-') {
        return Err(RefNameError::LeadingDash);
    }
    for reserved in ["HEAD", "@"] {
        if name == reserved {
            return Err(RefNameError::Reserved(reserved));
        }
    }
    check_ref_format(name)
}

/// The rules of `check_refname_format` for a name below `refs/heads/` or
/// `refs/tags/`.
fn check_ref_format(name: &str) -> Result<(), RefNameError> {
    for c in name.chars() {
        if c == ' ' {
            return Err(RefNameError::Space);
        }
        if c.is_ascii_control() {
            return Err(RefNameError::Control);
        }
        if FORBIDDEN.contains(&c) {
            return Err(RefNameError::Forbidden(c));
        }
    }
    if name.contains("..") {
        return Err(RefNameError::DoubleDot);
    }
    if name.contains("@{") {
        return Err(RefNameError::AtBrace);
    }
    for part in name.split('/') {
        if part.is_empty() {
            return Err(RefNameError::EmptyPart);
        }
        if part.starts_with('.') {
            return Err(RefNameError::DotPart);
        }
        if part.ends_with(".lock") {
            return Err(RefNameError::LockPart);
        }
    }
    if name.ends_with('.') {
        return Err(RefNameError::TrailingDot);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{RefNameError, check_branch_name};

    #[test]
    fn sec_4_each_rule_names_itself() {
        let cases = [
            ("", RefNameError::Empty),
            ("-x", RefNameError::LeadingDash),
            ("HEAD", RefNameError::Reserved("HEAD")),
            ("@", RefNameError::Reserved("@")),
            ("a b", RefNameError::Space),
            ("a\tb", RefNameError::Control),
            ("a~1", RefNameError::Forbidden('~')),
            ("a..b", RefNameError::DoubleDot),
            ("a@{1}", RefNameError::AtBrace),
            ("a//b", RefNameError::EmptyPart),
            ("a/.b", RefNameError::DotPart),
            ("a.lock/b", RefNameError::LockPart),
            ("a.", RefNameError::TrailingDot),
        ];
        for (name, error) in cases {
            assert_eq!(check_branch_name(name), Err(error), "{name:?}");
        }
        assert_eq!(
            RefNameError::Forbidden(':').to_string(),
            "the name cannot contain ':'"
        );
        assert_eq!(check_branch_name("feature/x-1"), Ok(()));
    }
}
