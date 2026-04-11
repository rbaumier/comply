use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Weak cipher names (case-insensitive matching).
const WEAK_CIPHERS: &[&str] = &["des", "rc4", "rc2", "blowfish"];

/// Crypto context prefixes that precede a cipher name.
const CRYPTO_CONTEXTS: &[&str] = &[
    "createcipheriv(",
    "createcipher(",
    "createdecipheriv(",
    "createdecipher(",
    "algorithm:",
    "cipher:",
    "algorithm =",
    "cipher =",
];

fn has_weak_cipher(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    for ctx in CRYPTO_CONTEXTS {
        if let Some(pos) = lower.find(ctx) {
            let rest = &lower[pos + ctx.len()..];
            for cipher in WEAK_CIPHERS {
                if rest.contains(cipher) {
                    return true;
                }
            }
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_weak_cipher(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-weak-cipher".into(),
                    message: "Weak cipher detected — use AES-256-GCM or ChaCha20-Poly1305."
                        .into(),
                    severity: Severity::Error,
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
    fn flags_createcipheriv_des() {
        assert_eq!(
            run("const c = crypto.createCipheriv('des-ecb', key, iv);").len(),
            1
        );
    }

    #[test]
    fn flags_algorithm_rc4() {
        assert_eq!(run("algorithm: 'rc4'").len(), 1);
    }

    #[test]
    fn flags_cipher_blowfish() {
        assert_eq!(run("cipher: 'blowfish'").len(), 1);
    }

    #[test]
    fn allows_aes_256_gcm() {
        assert!(run("const c = crypto.createCipheriv('aes-256-gcm', key, iv);").is_empty());
    }

    #[test]
    fn ignores_des_outside_crypto_context() {
        assert!(run("const description = 'some des text';").is_empty());
    }
}
