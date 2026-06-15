//! no-timing-attack Rust backend.
//!
//! Walks `binary_expression` nodes whose operator is `==` / `!=` and
//! flags the comparison if either operand refers to an identifier whose
//! normalized name is sensitive (see `is_sensitive_identifier`). Operands
//! that are string literals, call expressions, or any other shape are
//! ignored, so a string like `"index_signature"` is never inspected.

use crate::diagnostic::{Diagnostic, Severity};

use super::helpers::{is_content_integrity_comparison, is_sensitive_identifier};

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    if crate::rules::rust_helpers::is_in_test_context(node, source) {
        return;
    }
    if is_in_partial_eq_eq_method(node, source) {
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
    // A content-integrity / checksum comparison (e.g. a file's SHA-256 digest
    // against its expected value) compares public fingerprints, not secrets,
    // so it is not a timing-attack target.
    if is_content_integrity_comparison(left_name, right_name) {
        return;
    }
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

/// True if `node` sits inside the `eq` method of an `impl PartialEq for T`.
///
/// A `self.hash == other.hash` fast-path inside `PartialEq::eq` compares two
/// fields of the same `&Self` value — a structural-hash short-circuit, not a
/// secret check. There is no attacker-controlled input vs. stored-secret
/// asymmetry, so the timing-attack premise does not apply and the comparison is
/// exempt.
///
/// Walks up to the nearest enclosing `function_item`; the exemption applies only
/// when that method is named `eq` and its enclosing `impl_item` implements
/// `PartialEq` (bare or generic `PartialEq<Rhs>`, optionally path-qualified).
fn is_in_partial_eq_eq_method(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "function_item" {
            let is_eq = ancestor
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                == Some("eq");
            return is_eq && enclosing_impl_is_partial_eq(ancestor, source);
        }
        current = ancestor.parent();
    }
    false
}

/// True if the nearest `impl_item` enclosing `node` implements `PartialEq`.
fn enclosing_impl_is_partial_eq(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "impl_item" {
            let Some(trait_node) = ancestor.child_by_field_name("trait") else {
                return false;
            };
            let trait_text = trait_node.utf8_text(source).unwrap_or("");
            // Strip any `<Rhs>` generic args, then the trailing path segment, so
            // `PartialEq`, `PartialEq<Self>`, and `core::cmp::PartialEq` all match.
            let bare = trait_text
                .split('<')
                .next()
                .unwrap_or(trait_text)
                .rsplit("::")
                .next()
                .unwrap_or(trait_text)
                .trim();
            return bare == "PartialEq";
        }
        current = ancestor.parent();
    }
    false
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

    /// bevy_platform/src/hash.rs:95 — a structural-hash fast-path inside
    /// `PartialEq::eq`; both operands are fields of `&Self`, no secret.
    #[test]
    fn does_not_flag_hash_fastpath_in_partial_eq() {
        let src = r#"
impl<V: PartialEq, H> PartialEq for Hashed<V, H> {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.value.eq(&other.value)
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// bevy_diagnostic/src/diagnostic.rs:97 — same shape on `DiagnosticPath`.
    #[test]
    fn does_not_flag_hash_in_partial_eq_diagnostic_path() {
        let src = r#"
impl PartialEq for DiagnosticPath {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.path == other.path
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// Generic `PartialEq<Rhs>` is still the equality trait — exempt.
    #[test]
    fn does_not_flag_hash_in_generic_partial_eq() {
        let src = r#"
impl PartialEq<Other> for Thing {
    fn eq(&self, other: &Other) -> bool {
        self.hash == other.hash
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// Negative-space guard: a genuine secret comparison inside `eq` of a
    /// non-equality trait is NOT exempt — only `PartialEq::eq` is.
    #[test]
    fn flags_password_in_non_partial_eq_trait_eq_method() {
        let src = r#"
impl MyTrait for Thing {
    fn eq(&self, other: &Self) -> bool {
        self.password == other.password
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    /// Negative-space guard: a credential comparison in an inherent method (no
    /// trait) is NOT exempt — the exemption is scoped to `PartialEq::eq`.
    #[test]
    fn flags_secret_in_inherent_eq_method() {
        let src = r#"
impl Thing {
    fn eq(&self, other: &Self) -> bool {
        self.password_hash == other.password_hash
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    /// Negative-space guard: a secret comparison in a non-`eq` method of a
    /// `PartialEq` impl is NOT exempt — only the `eq` method short-circuit is.
    #[test]
    fn flags_password_in_partial_eq_non_eq_method() {
        let src = r#"
impl PartialEq for Thing {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
    fn check(&self, input: &str) -> bool {
        self.password == input
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    /// #3375: a bare `hash` is overloaded — a URL fragment / structural hash,
    /// not a credential — so a comparison of bare `hash` operands is exempt
    /// even outside a `PartialEq` impl.
    #[test]
    fn allows_bare_hash_comparison() {
        let src = "fn process(info: &Info, new_hash: u64) -> bool { info.hash == new_hash }";
        assert!(run_on(src).is_empty());
    }

    /// Over-exemption guard: a qualified cryptographic hash carries a crypto
    /// qualifier and stays flagged — only the bare `hash` is exempt.
    #[test]
    fn flags_qualified_crypto_hash_comparison() {
        let src = "fn f(password_hash: &str, expected_hash: &str) -> bool { password_hash == expected_hash }";
        assert_eq!(run_on(src).len(), 1);
    }

    /// A SHA-256 content-integrity check compares public fingerprints, so it
    /// is exempt even though one operand ends with `hash` (#3352).
    #[test]
    fn allows_sha256_integrity_comparison() {
        let src = "fn verify(sha256: &str, hash: &str) -> bool { sha256 != hash }";
        assert!(run_on(src).is_empty());
    }

    /// Over-exemption guard: a real credential comparison carries no integrity
    /// indicator and must still flag.
    #[test]
    fn flags_password_despite_integrity_exemption() {
        let src = "fn f(password: &str, input: &str) -> bool { password == input }";
        assert_eq!(run_on(src).len(), 1);
    }
}
