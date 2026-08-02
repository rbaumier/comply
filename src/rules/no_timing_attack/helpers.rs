//! Shared helpers for `no-timing-attack` — sensitive identifier match.

/// Words that unambiguously name a credential. A name ending with one of
/// these is treated as sensitive on its own. `totp` and `hotp` are listed
/// beside `otp` because a marker matches whole words only.
const SECRET_WORDS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "apikey",
    "auth",
    "hmac",
    "credential",
    "otp",
    "totp",
    "hotp",
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

/// Words that mark a value as a credential when paired with an ambiguous
/// role word.
const SECRET_INDICATORS: &[&str] = &[
    "password", "secret", "auth", "authorization", "authentication", "access", "refresh", "csrf",
    "xsrf", "bearer", "jwt", "session", "api", "oauth",
];

/// Words that pin an overloaded `hash` / `digest` name (see
/// `OVERLOADED_HASH_WORDS`) to its cryptographic sense. A name ending in one
/// of those words is sensitive only when one of these is also a word of the
/// name (`passwordHash`, `expectedHash`, `auth_digest`, `hmac_digest`), so a
/// bare `hash` (URL fragment) or `digest` (content hash) stays unflagged.
const HASH_CRYPTO_QUALIFIERS: &[&str] = &[
    "password", "passwd", "pwd", "secret", "credential", "token", "auth", "authorization",
    "authentication", "pin", "otp", "totp", "hotp", "key", "salt", "digest", "hmac", "sha", "md5",
    "bcrypt", "scrypt", "argon", "pbkdf", "signature", "checksum", "expected", "computed", "stored",
    "actual",
];

/// Words that name a cryptographic checksum in auth code (`passwordHash`,
/// `auth_digest`, `hmac_digest`) yet are equally the term for a public,
/// content-addressable value elsewhere: `hash` is the URL fragment
/// (`location.hash`, `route.hash`), `digest` is the canonical OCI / sigstore
/// content hash (`blob_digest`, a struct field `digest`, `digest.digest`). A
/// name ending in either is sensitive only when it also carries a
/// `HASH_CRYPTO_QUALIFIERS` word; the overloaded word never qualifies
/// itself, so `blob_digest` does not match on the `digest` qualifier.
const OVERLOADED_HASH_WORDS: &[&str] = &["hash", "digest"];

/// Words that mark a value as a *content-integrity* fingerprint — a checksum /
/// digest of file or download content, verified against a known (typically
/// public) value. Each names a specific digest algorithm or an integrity role,
/// so it cannot stand for a credential; the bare algorithm family `sha` is
/// excluded because it also names the signing primitive of an HMAC or a token
/// signature.
///
/// An entry only has to open a word, so the plural and participle forms
/// (`checksums`, `fingerprinting`) match too: a word that starts with one of
/// them names the same fingerprint.
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

/// An identifier lowercased with its separators removed, plus the offsets at
/// which its words begin and end. Every case convention collapses to the same
/// pair: `requires_hash` and `requiresHash` both give `"requireshash"` with
/// word boundaries `[0, 8, 12]`.
///
/// A marker is looked up against those boundaries, so a match can only start at
/// a word start: `contains_word` also ends on one, `contains_word_starting_with`
/// leaves the tail free. A free substring match instead matches letters that
/// span two words: `requires_hash` contains `sha` across the `require|sha|sh`
/// seam, and `spin` contains `pin`.
struct NormalizedName {
    text: String,
    boundaries: Vec<usize>,
}

impl NormalizedName {
    /// Splits `name` on separators (any non-alphanumeric character) and on the
    /// case / letter-digit transitions that mark a word start in camelCase,
    /// PascalCase, and SCREAMING_CASE: `APIKey` yields `api` + `key`, and
    /// `sha256Hash` yields `sha` + `256` + `hash`.
    fn new(name: &str) -> Self {
        let mut text = String::with_capacity(name.len());
        let mut boundaries = vec![0];
        let mut previous: Option<char> = None;
        let mut seen_separator = false;
        let mut chars = name.chars().peekable();
        while let Some(current) = chars.next() {
            if !current.is_alphanumeric() {
                seen_separator = true;
                continue;
            }
            let is_word_start = previous.is_some_and(|previous| {
                seen_separator
                    || (previous.is_lowercase() && current.is_uppercase())
                    || (previous.is_uppercase()
                        && current.is_uppercase()
                        && chars.peek().is_some_and(|next| next.is_lowercase()))
                    || (previous.is_alphabetic() && current.is_numeric())
                    || (previous.is_numeric() && current.is_alphabetic())
            });
            if is_word_start {
                boundaries.push(text.len());
            }
            text.extend(current.to_lowercase());
            seen_separator = false;
            previous = Some(current);
        }
        if boundaries.last() != Some(&text.len()) {
            boundaries.push(text.len());
        }
        Self { text, boundaries }
    }

    /// True when the name's trailing words spell `marker`: `user_password` ends
    /// with the word `password`, `spin` does not end with the word `pin`.
    fn ends_with_word(&self, marker: &str) -> bool {
        self.text.ends_with(marker) && self.is_boundary(self.text.len() - marker.len())
    }

    /// True when `marker` spells one or more whole words of the name:
    /// `sha256Hash` carries the word `sha`, `requires_hash` does not. Only a
    /// word start can open a match, so the scan walks the boundaries.
    fn contains_word(&self, marker: &str) -> bool {
        self.boundaries.iter().any(|&start| {
            self.text[start..].starts_with(marker) && self.is_boundary(start + marker.len())
        })
    }

    /// True when a word of the name starts with `marker`, so `file_checksums`
    /// carries `checksum` and `sha256s` carries `sha256`. The trailing letters
    /// stay unconstrained, which admits the plural and participle forms of a
    /// marker.
    fn contains_word_starting_with(&self, marker: &str) -> bool {
        self.boundaries
            .iter()
            .any(|&start| self.text[start..].starts_with(marker))
    }

    fn is_boundary(&self, offset: usize) -> bool {
        self.boundaries.contains(&offset)
    }
}

/// Returns true if `name` ends with a sensitive word. The rightmost word of an
/// identifier names the role of the value, so `user_password`, `userPassword`,
/// `USER_PASSWORD`, and `UserPassword` all match the `password` suffix, while
/// `token_type`, `hash_map_size`, and `auth_flow` do not (their last word is
/// `type` / `size` / `flow`).
///
/// Every marker is matched against the words of the identifier, never as a free
/// substring (see `NormalizedName`). A name that glues a marker into a longer
/// word (`authtoken`) or inflects it (`salted_hash`) therefore needs its own
/// entry, the way `totp` sits beside `otp`.
///
/// Ambiguous role words (`token`, `signature`) require an extra secret
/// indicator word in the name to fire, so `auth_token` and `api_signature`
/// match but a lexer's `comment_token` or an LSP `lsp_signature` does not.
///
/// A name ending in `hash` or `digest` requires a cryptographic qualifier word
/// (`passwordHash`, `expectedHash`, `auth_digest`, `hmac_digest`); a bare
/// `hash` (URL fragment) or `digest` (OCI / sigstore content hash) does not
/// match.
pub fn is_sensitive_identifier(name: &str) -> bool {
    let name = NormalizedName::new(name);
    if SECRET_WORDS.iter().any(|word| name.ends_with_word(word)) {
        return true;
    }
    if let Some(&word) = OVERLOADED_HASH_WORDS
        .iter()
        .find(|&&word| name.ends_with_word(word))
    {
        return HASH_CRYPTO_QUALIFIERS
            .iter()
            .any(|&qualifier| qualifier != word && name.contains_word(qualifier));
    }
    AMBIGUOUS_ROLE_WORDS
        .iter()
        .any(|word| name.ends_with_word(word))
        && SECRET_INDICATORS
            .iter()
            .any(|indicator| name.contains_word(indicator))
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

/// True if a content-integrity indicator opens a word of `name`, so
/// `zippedSha256` carries `sha256` across `sha` + `256`, and the plural
/// `file_checksums` carries `checksum`.
fn has_integrity_indicator(name: &str) -> bool {
    let name = NormalizedName::new(name);
    INTEGRITY_INDICATORS
        .iter()
        .any(|indicator| name.contains_word_starting_with(indicator))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_sensitive_names() {
        assert!(is_sensitive_identifier("password"));
        assert!(is_sensitive_identifier("hmac"));
    }

    /// astral-sh/uv crates/uv-resolver/src/lock/mod.rs:666 (#6855) —
    /// `requires_hash` spells the qualifier `sha` only across the seam of its
    /// two words (`require|sha|sh`), so it carries no cryptographic qualifier.
    #[test]
    fn qualifier_split_across_words_does_not_qualify() {
        assert!(!is_sensitive_identifier("requires_hash"));
        assert!(!is_sensitive_identifier("requiresHash"));
        assert!(!is_sensitive_identifier("RequiresHash"));
        assert!(!is_sensitive_identifier("REQUIRES_HASH"));
        // Same accident on the `key` qualifier: `keyword` is one word.
        assert!(!is_sensitive_identifier("keyword_digest"));
        // `shared` is a single word, so it does not carry the `sha` qualifier.
        assert!(!is_sensitive_identifier("sharedHash"));
    }

    /// Over-exemption guard for the word-boundary match: a qualifier that is a
    /// whole word still fires, in every case convention. `sha256` and `md5` are
    /// also content-integrity indicators, so `shaDigest`, `argon2_hash`,
    /// `pbkdf2Hash`, `totp_hash` and `hotp_digest` are the entries that reach a
    /// diagnostic at the rule level.
    #[test]
    fn qualifier_as_whole_word_still_fires() {
        assert!(is_sensitive_identifier("sha256_hash"));
        assert!(is_sensitive_identifier("sha256Hash"));
        assert!(is_sensitive_identifier("SHA256_HASH"));
        assert!(is_sensitive_identifier("shaDigest"));
        assert!(is_sensitive_identifier("md5_hash"));
        assert!(is_sensitive_identifier("argon2_hash"));
        assert!(is_sensitive_identifier("pbkdf2Hash"));
        assert!(is_sensitive_identifier("totp_hash"));
        assert!(is_sensitive_identifier("hotp_digest"));
    }

    /// A marker that only prefixes a longer word does not match it, so `totp`,
    /// `hotp`, `authorization` and `authentication` get their own entry.
    #[test]
    fn marker_that_only_prefixes_a_word_needs_its_own_entry() {
        assert!(is_sensitive_identifier("totp"));
        assert!(is_sensitive_identifier("user_totp"));
        assert!(is_sensitive_identifier("hotp"));
        assert!(is_sensitive_identifier("authorization_token"));
        assert!(is_sensitive_identifier("authenticationHash"));
    }

    /// A secret word buried inside a longer word is not that word: `spin` is
    /// not a `pin`, and `keyword` is not a `key`.
    #[test]
    fn secret_word_inside_longer_word_not_flagged() {
        assert!(!is_sensitive_identifier("spin"));
        assert!(!is_sensitive_identifier("current_spin"));
        assert!(!is_sensitive_identifier("keyword_hash"));
        assert!(!is_sensitive_identifier("unpin"));
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

    /// A plural or participle indicator names the same fingerprint, so a word
    /// that starts with an indicator marks the comparison too.
    #[test]
    fn inflected_integrity_indicator_detected() {
        assert!(is_content_integrity_comparison(
            Some("expected_hash"),
            Some("file_checksums")
        ));
        assert!(is_content_integrity_comparison(Some("etags"), Some("hash")));
        assert!(is_content_integrity_comparison(
            Some("fingerprinting"),
            Some("hash")
        ));
    }

    /// A genuine credential comparison carries no integrity indicator and is
    /// not treated as a content-integrity check.
    #[test]
    fn credential_comparison_not_integrity() {
        assert!(!is_content_integrity_comparison(Some("password"), Some("input")));
        assert!(!is_content_integrity_comparison(Some("authToken"), Some("expected")));
        assert!(!is_content_integrity_comparison(Some("hash"), Some("input")));
        // `sha` names the signing primitive of an HMAC or a token signature, so
        // it is not an integrity indicator and a shared secret stays a
        // credential comparison.
        assert!(!is_content_integrity_comparison(Some("sharedSecret"), Some("x")));
    }
}
