//! Shared helpers for `no-timing-attack` — sensitive identifier match.

/// Words that unambiguously name a credential. A name ending with one of
/// these is treated as sensitive on its own.
const SECRET_WORDS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "apikey",
    "auth",
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

/// Substrings that pin an overloaded `hash` / `digest` name (see
/// `OVERLOADED_HASH_WORDS`) to its cryptographic sense. A name ending in one
/// of those words is sensitive only when it also contains one of these
/// (`passwordHash`, `expectedHash`, `auth_digest`, `hmac_digest`), so a bare
/// `hash` (URL fragment) or `digest` (content hash) stays unflagged.
const HASH_CRYPTO_QUALIFIERS: &[&str] = &[
    "password", "passwd", "pwd", "secret", "credential", "token", "auth", "pin", "otp", "key",
    "salt", "digest", "hmac", "sha", "md5", "bcrypt", "scrypt", "argon", "pbkdf", "signature",
    "checksum", "expected", "computed", "stored", "actual",
];

/// Words that name a cryptographic checksum in auth code (`passwordHash`,
/// `auth_digest`, `hmac_digest`) yet are equally the term for a public,
/// content-addressable value elsewhere: `hash` is the URL fragment
/// (`location.hash`, `route.hash`), `digest` is the canonical OCI / sigstore
/// content hash (`blob_digest`, a struct field `digest`, `digest.digest`). A
/// name ending in either is sensitive only when it also carries a
/// `HASH_CRYPTO_QUALIFIERS` substring; the overloaded word never qualifies
/// itself, so `blob_digest` does not match on the `digest` qualifier.
const OVERLOADED_HASH_WORDS: &[&str] = &["hash", "digest"];

/// Substrings that mark a value as a *content-integrity* fingerprint — a
/// checksum / digest of file or download content, verified against a known
/// (typically public) value. These are distinctive enough to match as
/// substrings after normalization without colliding with credential names
/// (`sha` alone is excluded because it is a substring of `shared`/`sharedSecret`).
const INTEGRITY_INDICATORS: &[&str] = &[
    "sha1",
    "sha224",
    "sha256",
    "sha384",
    "sha512",
    "sha3",
    "md5",
    "checksum",
    "crc32",
    "integrity",
    "etag",
    "fingerprint",
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
///
/// A name ending in `hash` or `digest` requires a cryptographic qualifier
/// (`passwordHash`, `expectedHash`, `auth_digest`, `hmac_digest`); a bare
/// `hash` (URL fragment) or `digest` (OCI / sigstore content hash) does not
/// match.
pub fn is_sensitive_identifier(name: &str) -> bool {
    let normalized: String = name
        .chars()
        .filter(|c| *c != '_')
        .flat_map(|c| c.to_lowercase())
        .collect();
    if SECRET_WORDS.iter().any(|word| normalized.ends_with(word)) {
        return true;
    }
    if let Some(&word) = OVERLOADED_HASH_WORDS
        .iter()
        .find(|&&word| normalized.ends_with(word))
    {
        return HASH_CRYPTO_QUALIFIERS
            .iter()
            .any(|&qualifier| qualifier != word && normalized.contains(qualifier));
    }
    AMBIGUOUS_ROLE_WORDS
        .iter()
        .any(|word| normalized.ends_with(word))
        && SECRET_INDICATORS
            .iter()
            .any(|indicator| normalized.contains(indicator))
}

/// Returns true when a comparison of operands named `left` / `right` is a
/// content-integrity check rather than a secret-equality check.
///
/// A `hash` / `digest` name is overloaded: in an auth context it names a
/// stored credential, but in download / file-verification code it names a
/// SHA-256 (or other) checksum of public content. Such a digest is a
/// deterministic, public fingerprint — neither operand is secret, and an
/// attacker who cannot supply the content gains nothing by measuring
/// comparison time. The check fires when *either* operand name carries a
/// content-integrity indicator (`sha256`, `md5`, `checksum`, `etag`, …),
/// covering the idiom where the expected side is named for the algorithm
/// (`sha256`) and the computed side is a bare `hash`.
///
/// A genuine credential comparison (`password === input`, `authToken ==
/// expected`) carries no integrity indicator and is not exempted.
pub fn is_content_integrity_comparison(left: Option<&str>, right: Option<&str>) -> bool {
    [left, right]
        .into_iter()
        .flatten()
        .any(has_integrity_indicator)
}

/// True if `name`, after normalization (lowercase + strip `_`), contains a
/// content-integrity indicator.
fn has_integrity_indicator(name: &str) -> bool {
    let normalized: String = name
        .chars()
        .filter(|c| *c != '_')
        .flat_map(|c| c.to_lowercase())
        .collect();
    INTEGRITY_INDICATORS
        .iter()
        .any(|indicator| normalized.contains(indicator))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_sensitive_names() {
        assert!(is_sensitive_identifier("password"));
        assert!(is_sensitive_identifier("hmac"));
    }

    /// `digest` is overloaded exactly like `hash`: a cryptographic digest in
    /// auth code, but the canonical content-addressable SHA-256 term in
    /// OCI / sigstore tooling (#6809). It fires only with a crypto qualifier.
    #[test]
    fn digest_needs_crypto_qualifier() {
        // Credential-qualified digests still fire.
        assert!(is_sensitive_identifier("auth_digest"));
        assert!(is_sensitive_identifier("password_digest"));
        assert!(is_sensitive_identifier("hmac_digest"));
        assert!(is_sensitive_identifier("expected_digest"));
        // A bare or content-addressed `digest` is a public fingerprint, not a
        // credential — the overloaded word does not qualify itself.
        assert!(!is_sensitive_identifier("digest"));
        assert!(!is_sensitive_identifier("blob_digest"));
        assert!(!is_sensitive_identifier("messageDigest"));
    }

    /// `hash` is overloaded: a cryptographic digest in auth code, a URL
    /// fragment in routing code. It fires only with a crypto qualifier.
    #[test]
    fn hash_needs_crypto_qualifier() {
        // Genuine crypto hashes still fire.
        assert!(is_sensitive_identifier("passwordHash"));
        assert!(is_sensitive_identifier("password_hash"));
        assert!(is_sensitive_identifier("expected_hash"));
        assert!(is_sensitive_identifier("expectedHash"));
        assert!(is_sensitive_identifier("computedHash"));
        assert!(is_sensitive_identifier("sha256Hash"));
        assert!(is_sensitive_identifier("token_hash"));
        // A bare or routing `hash` is the URL fragment, not a credential.
        assert!(!is_sensitive_identifier("hash"));
        assert!(!is_sensitive_identifier("locationHash"));
        assert!(!is_sensitive_identifier("routeHash"));
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

    /// A checksum indicator on either operand marks the comparison as a
    /// content-integrity check (the prisma `sha256 !== hash` FP, #3352).
    #[test]
    fn integrity_comparison_detected() {
        assert!(is_content_integrity_comparison(Some("sha256"), Some("hash")));
        assert!(is_content_integrity_comparison(
            Some("zippedSha256"),
            Some("zippedHash")
        ));
        assert!(is_content_integrity_comparison(Some("checksum"), Some("expected")));
        assert!(is_content_integrity_comparison(Some("md5Digest"), Some("computed")));
        assert!(is_content_integrity_comparison(Some("file_etag"), Some("remote")));
    }

    /// A genuine credential comparison carries no integrity indicator and is
    /// not treated as a content-integrity check.
    #[test]
    fn credential_comparison_not_integrity() {
        assert!(!is_content_integrity_comparison(Some("password"), Some("input")));
        assert!(!is_content_integrity_comparison(Some("authToken"), Some("expected")));
        assert!(!is_content_integrity_comparison(Some("hash"), Some("input")));
        // `sha` is a substring of `shared` but is excluded as an indicator, so
        // a shared secret is still treated as a credential comparison.
        assert!(!is_content_integrity_comparison(Some("sharedSecret"), Some("x")));
    }
}
