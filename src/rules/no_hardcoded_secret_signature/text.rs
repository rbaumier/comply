use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Well-known Web Crypto API algorithm identifiers. These are public standard
/// names per the W3C Web Crypto spec, never secrets, so a string literal equal
/// to one of them is never a hardcoded credential.
const WEB_CRYPTO_ALGORITHMS: &[&str] = &[
    "RSASSA-PKCS1-v1_5",
    "RSA-PSS",
    "RSA-OAEP",
    "ECDSA",
    "ECDH",
    "HMAC",
    "AES-CTR",
    "AES-CBC",
    "AES-GCM",
    "AES-KW",
    "SHA-1",
    "SHA-256",
    "SHA-384",
    "SHA-512",
    "PBKDF2",
    "HKDF",
    "Ed25519",
    "X25519",
];

fn is_web_crypto_algorithm(s: &str) -> bool {
    WEB_CRYPTO_ALGORITHMS
        .iter()
        .any(|alg| alg.eq_ignore_ascii_case(s))
}

/// Returns true if a string literal (between quotes) looks like a secret:
/// alphanumeric (with common secret chars like +/=_-) and longer than 8 chars.
fn looks_like_secret(s: &str) -> bool {
    s.len() > 8
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '_' | '-'))
}

/// Extract the content between the first pair of quotes (single or double) after
/// `start`, returning the matched string and the byte index in `line` where its
/// opening quote sits.
fn extract_quoted_string(line: &str, start: usize) -> Option<(&str, usize)> {
    let rest = &line[start..];
    for quote in ['"', '\''] {
        if let Some(open) = rest.find(quote) {
            let after_open = open + 1;
            if let Some(close) = rest[after_open..].find(quote) {
                return Some((&rest[after_open..after_open + close], start + open));
            }
        }
    }
    None
}

/// Object-property names that carry the message/plaintext to be signed rather
/// than credentials (`openpgp.sign({ text: 'plaintext' })`). A string value
/// assigned to one of these is content, not a secret. Secret-bearing property
/// names (`key`, `secret`, `privateKey`, ...) are deliberately absent so a
/// hardcoded secret passed as an object property is still flagged.
const MESSAGE_PROPERTY_NAMES: &[&str] =
    &["text", "message", "data", "payload", "plaintext", "content"];

/// Returns true if the string opening at `quote_pos` is the value of a
/// message-carrying object property (`{ text: 'plaintext' }`). The property name
/// is the identifier immediately before the `:` preceding the quote.
fn is_message_property_value(line: &str, quote_pos: usize) -> bool {
    let before = line[..quote_pos].trim_end();
    let Some(name) = before.strip_suffix(':') else {
        return false;
    };
    let name = name.trim_end();
    let name_start = name
        .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
        .map_or(0, |i| i + 1);
    let prop = &name[name_start..];
    MESSAGE_PROPERTY_NAMES
        .iter()
        .any(|m| m.eq_ignore_ascii_case(prop))
}

fn has_hardcoded_secret(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();

    // Look for .sign( or .verify( calls
    for func in [".sign(", ".verify("] {
        let mut search_from = 0;
        while let Some(pos) = lower[search_from..].find(func) {
            let abs = search_from + pos + func.len();
            // Look for a string literal argument (the secret/key) in the arguments
            if let Some((secret, quote_pos)) = extract_quoted_string(line, abs)
                && looks_like_secret(secret)
                && !is_web_crypto_algorithm(secret)
                && !is_message_property_value(line, quote_pos)
            {
                return true;
            }
            search_from = abs;
        }
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_hardcoded_secret(line) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-hardcoded-secret-signature".into(),
                    message:
                        "Hardcoded secret in signing/verification — use env vars or a secrets manager."
                            .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_jwt_sign_with_hardcoded_secret() {
        assert_eq!(
            run("const token = jwt.sign(payload, 'mySuperSecretKey123');").len(),
            1
        );
    }

    #[test]
    fn flags_verify_with_hardcoded_secret() {
        assert_eq!(
            run("const decoded = jwt.verify(token, 'aVeryLongSecretString');").len(),
            1
        );
    }

    #[test]
    fn allows_sign_with_variable() {
        assert!(run("const token = jwt.sign(payload, process.env.SECRET);").is_empty());
    }

    #[test]
    fn allows_sign_with_short_string() {
        // Short strings (<=8 chars) are not flagged — likely not secrets
        assert!(run("const token = jwt.sign(payload, 'test');").is_empty());
    }

    #[test]
    fn ignores_non_crypto_sign() {
        assert!(run("document.sign('hello');").is_empty());
    }

    #[test]
    fn allows_web_crypto_algorithm_identifier_in_sign() {
        // webCrypto.sign(algorithm, key, data): the first arg is a public
        // algorithm identifier, not a secret.
        assert!(
            run("return new Uint8Array(await webCrypto.sign('RSASSA-PKCS1-v1_5', key, data));")
                .is_empty()
        );
    }

    #[test]
    fn allows_web_crypto_algorithm_identifier_in_verify() {
        assert!(run("return webCrypto.verify('RSASSA-PKCS1-v1_5', key, s, data);").is_empty());
    }

    #[test]
    fn allows_subtle_sign_with_string_args() {
        // crypto.subtle.sign(algorithm, key, data): no positional arg is a secret.
        assert!(run("await crypto.subtle.sign('HMAC', key, data);").is_empty());
    }

    #[test]
    fn allows_plaintext_message_object_property() {
        // The string is an object property value (the message text), not a
        // positional secret argument.
        assert!(run("await openpgp.sign({ message: msg, text: 'plaintext' });").is_empty());
    }

    #[test]
    fn still_flags_hardcoded_hmac_key_after_algorithm() {
        // A genuine positional string secret is still flagged even when an
        // algorithm identifier appears earlier in the call.
        assert_eq!(
            run("const t = jwt.sign(payload, 'mySuperSecretKey123', { algorithm: 'HS256' });")
                .len(),
            1
        );
    }

    #[test]
    fn still_flags_hardcoded_secret_in_object_property() {
        // A secret passed as a `secret`/`key` object property is still a
        // hardcoded credential — only message-carrying property names are exempt.
        assert_eq!(
            run("const t = jwt.sign(payload, { secret: 'mySuperSecretKey123' });").len(),
            1
        );
        assert_eq!(
            run("await webCrypto.verify({ key: 'mySuperSecretKey123' }, sig, data);").len(),
            1
        );
    }
}
