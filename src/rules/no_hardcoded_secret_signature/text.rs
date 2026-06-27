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

/// Returns true if the string opening at `quote_pos` is the argument of a nested
/// call sitting between the sign/verify open-paren (`args_start`) and the string,
/// e.g. `ecc.sign(h('5e9f…'), …)`. There the hex is decoded to bytes by `h(…)` —
/// it is not a bare credential string passed directly to the sign/verify call, so
/// it is a converted value (test-vector message hash / key material), not a
/// hardcoded secret. `args_start` is the byte index just after the `.sign(` /
/// `.verify(` token; a real positional secret (`jwt.sign(payload, 'secret')`) has
/// no identifier-opened `(` between that point and its opening quote.
fn is_nested_call_value(line: &str, args_start: usize, quote_pos: usize) -> bool {
    let before = line[args_start..quote_pos].trim_end();
    let Some(prefix) = before.strip_suffix('(') else {
        return false;
    };
    prefix
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
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
                && !is_nested_call_value(line, abs, quote_pos)
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
    fn allows_hex_decoded_test_vector_in_ecc_sign() {
        // bitcoinjs/bip32 testecc.ts: the hex is decoded to bytes by `h(...)`, a
        // message hash / key in an ECC test vector, not a bare credential string.
        assert!(
            run("assert(tools.compare(ecc.sign(h('5e9f0a0d593efdcf78ac923bc3313e4e7d408d574354ee2b3288c0da9fbba6ed'), h('fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364140')), h('54c4a33c6423d689378f160a7ff8b61330444abb58fb470f96ea16d99d4a2fed07082304410efa6b2943111b6a4e0aaa7b7db55a07e9861d1fb3cb1f421044a5')) === 0);")
                .is_empty()
        );
    }

    #[test]
    fn allows_hex_decoded_test_vector_in_ecc_verify() {
        assert!(
            run("assert(ecc.verify(h('5e9f0a0d593efdcf78ac923bc3313e4e7d408d574354ee2b3288c0da9fbba6ed'), h('0379be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798'), h('54c4a33c6423d689378f160a7ff8b61330444abb58fb470f96ea16d99d4a2fed07082304410efa6b2943111b6a4e0aaa7b7db55a07e9861d1fb3cb1f421044a5')));")
                .is_empty()
        );
    }

    #[test]
    fn allows_wif_test_vector_decoded_in_psbt_sign() {
        // scure-btc-signer bip174-psbt.test.ts: the WIF string is the argument of
        // a nested `btc.WIF(testnet).decode(...)` call, not a bare positional
        // credential passed straight to `.sign(...)`. It is decoded to key bytes,
        // a BIP-174 spec test vector, so it is a converted value, not a hardcoded
        // secret.
        assert!(
            run("tx4.sign(btc.WIF(testnet).decode('cP53pDbR5WtAD8dYAW9hhTjuvvTVaEiQBdrz9XPrgLBeRFiyCbQr'));")
                .is_empty()
        );
        assert!(
            run("tx5.sign(btc.WIF(testnet).decode('cT7J9YpCwY3AVRFSjN6ukeEeWY6mhpbJPxRaDaP5QTdygQRxP9Au'));")
                .is_empty()
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
