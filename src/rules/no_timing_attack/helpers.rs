//! Shared helpers for `no-timing-attack` — sensitive identifier match.

const SENSITIVE_WORDS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "apikey",
    "auth",
    "hash",
    "digest",
    "signature",
    "hmac",
    "credential",
    "otp",
    "pin",
];

/// Returns true if `name` ends with a sensitive word after normalization
/// (lowercase + remove `_` so both snake_case and camelCase collapse to
/// the same form). A full-name suffix match captures the convention
/// that the rightmost token is the role/type of a variable, so
/// `user_password`, `userPassword`, `USER_PASSWORD`, and `UserPassword`
/// all collapse to "userpassword" and match the "password" suffix,
/// while `token_type`, `hash_map_size`, and `auth_flow` do not match
/// because their suffix is `type` / `size` / `flow`.
pub fn is_sensitive_identifier(name: &str) -> bool {
    let normalized: String = name
        .chars()
        .filter(|c| *c != '_')
        .flat_map(|c| c.to_lowercase())
        .collect();
    SENSITIVE_WORDS.iter().any(|word| normalized.ends_with(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_sensitive_names() {
        assert!(is_sensitive_identifier("password"));
        assert!(is_sensitive_identifier("hash"));
        assert!(is_sensitive_identifier("signature"));
        assert!(is_sensitive_identifier("token"));
    }

    #[test]
    fn snake_case_suffix() {
        assert!(is_sensitive_identifier("user_password"));
        assert!(is_sensitive_identifier("expected_hash"));
        assert!(is_sensitive_identifier("api_key"));
        assert!(is_sensitive_identifier("session_token"));
    }

    #[test]
    fn camel_case_suffix() {
        assert!(is_sensitive_identifier("userPassword"));
        assert!(is_sensitive_identifier("expectedHash"));
        assert!(is_sensitive_identifier("sessionToken"));
    }

    #[test]
    fn upper_snake_case() {
        assert!(is_sensitive_identifier("API_KEY"));
        assert!(is_sensitive_identifier("USER_PASSWORD"));
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
