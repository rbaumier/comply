//! no-timing-attack Rust backend.
//!
//! Walks `binary_expression` nodes whose operator is `==` / `!=` and
//! flags the comparison if either operand refers to an identifier whose
//! normalized name is sensitive (see `is_sensitive_identifier`). Operands
//! that are string literals, call expressions, or any other shape are
//! ignored, so a string like `"index_signature"` is never inspected.

use crate::diagnostic::{Diagnostic, Severity};

use super::helpers::is_sensitive_identifier;

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    if crate::rules::rust_helpers::is_in_test_context(node, source) {
        return;
    }
    let Some(op) = node.child_by_field_name("operator") else { return };
    let op_text = op.utf8_text(source).unwrap_or("");
    if op_text != "==" && op_text != "!=" {
        return;
    }
    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    let left_name = operand_name(left, source);
    let right_name = operand_name(right, source);
    let left_hit = left_name.is_some_and(is_sensitive_identifier);
    let right_hit = right_name.is_some_and(is_sensitive_identifier);
    if !left_hit && !right_hit {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-timing-attack".into(),
        message: "Direct comparison of a security-sensitive value \u{2014} use a constant-time comparison (`constant_time_eq::constant_time_eq`, `subtle::ConstantTimeEq`).".into(),
        severity: Severity::Error,
        span: None,
    });
}

fn operand_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "identifier" => node.utf8_text(source).ok(),
        "field_expression" => node
            .child_by_field_name("field")
            .and_then(|f| f.utf8_text(source).ok()),
        _ => None,
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
    fn flags_password_comparison() {
        let src = "fn f(password: &str, input: &str) -> bool { password == input }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_auth_token_comparison() {
        let src = "fn f() -> bool { auth_token == expected_auth_token }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_field_expression_password() {
        // `user.password_hash == input` — left is a field_expression.
        let src = "fn f(user: &User, input: &str) -> bool { user.password_hash == input }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_hash_comparison() {
        let src = "fn f() -> bool { expected_hash != received_hash }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_api_key_snake_case() {
        // Normalized form is "apikey" — ends_with "apikey".
        let src = "fn f() -> bool { supplied_api_key == known_api_key }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_non_sensitive_comparison() {
        let src = "fn f() -> bool { name == other }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_token_type_lexer() {
        // `token_type` normalizes to "tokentype" — ends with "type",
        // not sensitive.
        let src = "fn f() -> bool { token_type == other_type }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_hashmap_size() {
        let src = "fn f() -> bool { hashmap_size == 0 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_signature_bytes_count() {
        let src = "fn f() -> bool { signature_bytes != 64 }";
        assert!(run_on(src).is_empty());
    }

    /// The exact FP observed during the walkthrough: a string literal
    /// `"index_signature"` (tree-sitter node kind) compared via `!=`.
    #[test]
    fn does_not_flag_string_literal_containing_signature() {
        let src = r#"
fn check(member: tree_sitter::Node) {
    if member.kind() != "index_signature" {
        return;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_call_expression_operand() {
        // `member.kind() != "foo"` — left is a call_expression, right is
        // a string literal. Neither is inspected.
        let src = r#"
fn check(member: tree_sitter::Node) {
    let _ = member.kind() != "password";
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// helix-core/src/comment.rs:63 — `token` is a comment-syntax marker
    /// (`//`, `#`, …), not an auth token.
    #[test]
    fn does_not_flag_comment_syntax_token() {
        let src = "fn f(fragment: &str, token: &str) -> bool { fragment != token }";
        assert!(run_on(src).is_empty());
    }

    /// helix-term/src/commands.rs:5305 — `current_comment_token` is the
    /// active comment prefix in the editor.
    #[test]
    fn does_not_flag_current_comment_token() {
        let src =
            "fn f(token: &str, current_comment_token: Option<&str>) -> bool { Some(token) == current_comment_token }";
        assert!(run_on(src).is_empty());
    }

    /// helix-term/src/handlers/signature_help.rs:253 — `lsp_signature` is
    /// an LSP function-call signature, not a digital signature.
    #[test]
    fn does_not_flag_lsp_signature() {
        let src = "fn f(old_lsp_sig: &Sig, lsp_signature: &Sig) -> bool { old_lsp_sig != lsp_signature }";
        assert!(run_on(src).is_empty());
    }
}
