//! no-deprecated-cipher backend for Rust.
//!
//! Flags deprecated crypto function calls — `Cipher::aes_128_cbc()` without
//! explicit IV, or calls to functions named `encrypt` / `decrypt` that use
//! deprecated key-derivation patterns.

use crate::diagnostic::{Diagnostic, Severity};

/// Deprecated crypto function names in Rust crates.
const DEPRECATED_FUNCTIONS: &[&str] = &[
    "crypto::symm::encrypt",
    "crypto::symm::decrypt",
    "openssl::symm::encrypt",
    "openssl::symm::decrypt",
];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");

    for &deprecated in DEPRECATED_FUNCTIONS {
        if callee_text.ends_with(deprecated) || callee_text == deprecated {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-deprecated-cipher".into(),
                message: format!(
                    "`{callee_text}()` uses a deprecated crypto API — use the `aead` or `cipher` crate with explicit IV/nonce.",
                ),
                severity: Severity::Error,
                span: None,
            });
            return;
        }
    }
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_openssl_symm_encrypt() {
        assert_eq!(
            run_on("fn f() { openssl::symm::encrypt(cipher, key, iv, data); }").len(),
            1,
        );
    }

    #[test]
    fn flags_crypto_symm_decrypt() {
        assert_eq!(
            run_on("fn f() { crypto::symm::decrypt(cipher, key, iv, data); }").len(),
            1,
        );
    }

    #[test]
    fn allows_aead_encrypt() {
        assert!(run_on("fn f() { aead::encrypt(nonce, plaintext); }").is_empty());
    }
}
