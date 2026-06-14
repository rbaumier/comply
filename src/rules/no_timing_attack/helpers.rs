//! Shared helpers for `no-timing-attack` — sensitive identifier match.

/// Words that unambiguously name a credential. A name ending with one of
/// these is treated as sensitive on its own.
const SECRET_WORDS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "apikey",
    "auth",
    "hash",
    "digest",
    "hmac",
    "credential",
    "otp",
    "pin",
];

/// Words that name a *role* shared by security and non-security domains:
/// `token` also means a lexer / comment-syntax token, `signature` also
/// means an LSP / function-call signature. A name ending with one of
/// these is only sensitive when the name also carries an explicit secret
/// indicator (`auth_token`, `access_token`, `api_signature`), so a
/// parser's `comment_token` or a language server's `lsp_signature` is not
/// flagged.
const AMBIGUOUS_ROLE_WORDS: &[&str] = &["token", "signature"];

/// Substrings that mark a value as a credential when paired with an
/// ambiguous role word.
const SECRET_INDICATORS: &[&str] = &[
    "password", "secret", "auth", "access", "refresh", "csrf", "xsrf", "bearer", "jwt", "session",
    "api", "oauth",
];

/// Returns true if `name` ends with a sensitive word after normalization
/// (lowercase + remove `_` so both snake_case and camelCase collapse to
/// the same form). A full-name suffix match captures the convention that
/// the rightmost word is the role/type of a variable, so `user_password`,
/// `userPassword`, `USER_PASSWORD`, and `UserPassword` all collapse to
/// "userpassword" and match the "password" suffix, while `token_type`,
/// `hash_map_size`, and `auth_flow` do not (their suffix is
/// `type` / `size` / `flow`).
///
/// Ambiguous role words (`token`, `signature`) require an extra secret
/// indicator in the name to fire, so `auth_token` and `api_signature`
/// match but a lexer's `comment_token` or an LSP `lsp_signature` does not.
pub fn is_sensitive_identifier(name: &str) -> bool {
    let normalized: String = name
        .chars()
        .filter(|c| *c != '_')
        .flat_map(|c| c.to_lowercase())
        .collect();
    if SECRET_WORDS.iter().any(|word| normalized.ends_with(word)) {
        return true;
    }
    AMBIGUOUS_ROLE_WORDS
        .iter()
        .any(|word| normalized.ends_with(word))
        && SECRET_INDICATORS
            .iter()
            .any(|indicator| normalized.contains(indicator))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_sensitive_names() {
        assert!(is_sensitive_identifier("password"));
        assert!(is_sensitive_identifier("hash"));
    }

    #[test]
    fn snake_case_suffix() {
        assert!(is_sensitive_identifier("user_password"));
        assert!(is_sensitive_identifier("expected_hash"));
        assert!(is_sensitive_identifier("api_key"));
        assert!(is_sensitive_identifier("auth_token"));
    }

    #[test]
    fn camel_case_suffix() {
        assert!(is_sensitive_identifier("userPassword"));
        assert!(is_sensitive_identifier("expectedHash"));
        assert!(is_sensitive_identifier("accessToken"));
    }

    #[test]
    fn upper_snake_case() {
        assert!(is_sensitive_identifier("API_KEY"));
        assert!(is_sensitive_identifier("USER_PASSWORD"));
    }

    /// `token` / `signature` are role words shared with lexers and LSPs;
    /// they only count as secrets when an indicator (`auth`, `access`,
    /// `api`, …) is also present.
    #[test]
    fn ambiguous_role_words_need_indicator() {
        // Genuine credentials still fire.
        assert!(is_sensitive_identifier("auth_token"));
        assert!(is_sensitive_identifier("access_token"));
        assert!(is_sensitive_identifier("refreshToken"));
        assert!(is_sensitive_identifier("csrf_token"));
        assert!(is_sensitive_identifier("api_token"));
        assert!(is_sensitive_identifier("api_signature"));
        // Non-security uses of the same role words do not.
        assert!(!is_sensitive_identifier("token"));
        assert!(!is_sensitive_identifier("comment_token"));
        assert!(!is_sensitive_identifier("current_comment_token"));
        assert!(!is_sensitive_identifier("signature"));
        assert!(!is_sensitive_identifier("lsp_signature"));
        assert!(!is_sensitive_identifier("old_lsp_sig"));
    }

    #[test]
    fn non_sensitive_suffix_not_flagged() {
        assert!(!is_sensitive_identifier("token_type"));
        assert!(!is_sensitive_identifier("hash_map_size"));
        assert!(!is_sensitive_identifier("signature_bytes"));
        assert!(!is_sensitive_identifier("auth_flow"));
        assert!(!is_sensitive_identifier("password_length"));
        assert!(!is_sensitive_identifier("hashmap_size"));
    }

    #[test]
    fn unrelated_names_not_flagged() {
        assert!(!is_sensitive_identifier("name"));
        assert!(!is_sensitive_identifier("other"));
        assert!(!is_sensitive_identifier("value"));
        assert!(!is_sensitive_identifier("index"));
    }
}
