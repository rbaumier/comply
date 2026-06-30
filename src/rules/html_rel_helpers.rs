//! Shared `rel` attribute semantics for the `*-no-target-blank` rules
//! (`react-jsx-no-target-blank`, `vue-no-target-blank`).

/// Whether a `rel` attribute value severs `window.opener` for a `target="_blank"` link.
///
/// The value is a space-separated token list (per the HTML spec). Either `noopener`
/// (which alone nulls `window.opener`) or `noreferrer` (which implies `noopener`)
/// closes the reverse-tabnabbing vector. Token order and unrelated tokens (e.g.
/// `nofollow`) are irrelevant; matching is case-insensitive and per-token, so a
/// single token that merely contains `noopener` as a substring (`notnoopener`)
/// does not count.
#[must_use]
pub fn rel_is_safe(value: &str) -> bool {
    value.split_ascii_whitespace().any(|token| {
        token.eq_ignore_ascii_case("noopener") || token.eq_ignore_ascii_case("noreferrer")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noopener_alone_is_safe() {
        assert!(rel_is_safe("noopener"));
    }

    #[test]
    fn noreferrer_alone_is_safe() {
        assert!(rel_is_safe("noreferrer"));
    }

    #[test]
    fn multi_token_with_safe_token_is_safe() {
        assert!(rel_is_safe("nofollow noopener noreferrer"));
    }

    #[test]
    fn case_insensitive() {
        assert!(rel_is_safe("NoOpener"));
    }

    #[test]
    fn unrelated_tokens_are_not_safe() {
        assert!(!rel_is_safe("nofollow"));
    }

    #[test]
    fn substring_trap_is_not_safe() {
        // A single token that merely contains `noopener` is not the `noopener` token.
        assert!(!rel_is_safe("notnoopener"));
    }

    #[test]
    fn empty_is_not_safe() {
        assert!(!rel_is_safe(""));
    }
}
