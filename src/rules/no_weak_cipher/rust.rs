//! no-weak-cipher backend for Rust.
//!
//! Flags weak ciphers (DES, RC4, RC2, Blowfish) in Rust crypto code.

use crate::diagnostic::{Diagnostic, Severity};

const WEAK_CIPHERS: &[&str] = &["des", "rc4", "rc2", "blowfish"];

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "string_literal" && kind != "raw_string_literal" {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    let lower = text.to_ascii_lowercase();

    // Check for weak cipher names in string literals that look like cipher specs
    // e.g. "des-ecb", "rc4", "blowfish-cbc"
    for cipher in WEAK_CIPHERS {
        if !lower.contains(cipher) {
            continue;
        }
        // Verify it looks like a cipher context (contains a dash separator or
        // is a known cipher identifier, not just "description" containing "des")
        let inner = if text.len() >= 2 { &text[1..text.len() - 1] } else { text };
        let inner_lower = inner.to_ascii_lowercase();
        if inner_lower == *cipher
            || inner_lower.starts_with(&format!("{cipher}-"))
            || inner_lower.starts_with(&format!("{cipher}_"))
            || inner_lower.contains(&format!("-{cipher}"))
        {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-weak-cipher".into(),
                message: "Weak cipher detected — use AES-256-GCM or ChaCha20-Poly1305.".into(),
                severity: Severity::Error,
            });
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_des_ecb() {
        assert_eq!(run_on(r#"fn f() { let c = "des-ecb"; }"#).len(), 1);
    }

    #[test]
    fn flags_rc4() {
        assert_eq!(run_on(r#"fn f() { let c = "rc4"; }"#).len(), 1);
    }

    #[test]
    fn allows_aes_256_gcm() {
        assert!(run_on(r#"fn f() { let c = "aes-256-gcm"; }"#).is_empty());
    }

    #[test]
    fn ignores_des_in_description() {
        assert!(run_on(r#"fn f() { let s = "description of thing"; }"#).is_empty());
    }
}
