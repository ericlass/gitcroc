//! Glob patterns as git matches them, for lists the user edits such as the
//! AI exclude list (AI-4).

use gix::bstr::ByteSlice;
use gix::glob::wildmatch::Mode;

/// Whether `pattern` matches `value` as git's wildmatch does, `*` and `?`
/// not matching `/`; case is ignored when `ignore_case`.
pub fn glob_match(pattern: &str, value: &[u8], ignore_case: bool) -> bool {
    let mut mode = Mode::NO_MATCH_SLASH_LITERAL;
    if ignore_case {
        mode |= Mode::IGNORE_CASE;
    }
    gix::glob::wildmatch(pattern.as_bytes().as_bstr(), value.as_bstr(), mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_4_globs_match_as_git_does() {
        assert!(glob_match(".env*", b".env.local", false));
        assert!(glob_match("*.pem", b"server.pem", false));
        assert!(!glob_match("*.pem", b"certs/server.pem", false));
        assert!(glob_match("certs/*", b"certs/server.pem", false));
        assert!(!glob_match("certs/*", b"certs/a/server.pem", false));
        assert!(!glob_match("*.KEY", b"tls.key", false));
        assert!(glob_match("*.KEY", b"tls.key", true));
    }
}
