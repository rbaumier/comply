//! no-weak-keys backend for Rust.
//!
//! Flags weak RSA key sizes (< 2048 bits) in integer literals used in
//! key-generation contexts.

use crate::diagnostic::{Diagnostic, Severity};

/// RSA key lengths considered weak.
const WEAK_RSA_LENGTHS: &[&str] = &["256", "384", "512", "768", "1024"];

crate::ast_check! { on ["integer_literal"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("");
    if !WEAK_RSA_LENGTHS.contains(&text) {
        return;
    }

    // Check context: look at the full line for key-generation patterns
    let line_idx = node.start_position().row;
    let full_text = std::str::from_utf8(source).unwrap_or("");
    let line = match full_text.lines().nth(line_idx) {
        Some(l) => l.to_ascii_lowercase(),
        None => return,
    };

    if line.contains("key_size")
        || line.contains("key_len")
        || line.contains("bits")
        || line.contains("modulus")
        || line.contains("rsa")
        || line.contains("key_bits")
    {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-weak-keys".into(),
            message: format!("Weak RSA key length ({text} bits) — use at least 2048 bits."),
            severity: Severity::Error,
            span: None,
        });
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
    fn flags_rsa_1024() {
        assert_eq!(run_on("fn f() { let key_size = 1024; }").len(), 1);
    }

    #[test]
    fn flags_rsa_512() {
        assert_eq!(run_on("fn f() { Rsa::generate(512).unwrap(); }").len(), 1);
    }

    #[test]
    fn allows_rsa_2048() {
        assert!(run_on("fn f() { let key_size = 2048; }").is_empty());
    }

    #[test]
    fn allows_non_key_integer() {
        assert!(run_on("fn f() { let port = 1024; }").is_empty());
    }
}
