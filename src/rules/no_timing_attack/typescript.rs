//! no-timing-attack TypeScript / JavaScript / TSX backend.
//!
//! Walks `binary_expression` nodes whose operator is `===` / `!==` /
//! `==` / `!=` and flags the comparison if either operand is an
//! identifier or member-access whose normalized last name is sensitive
//! (see `is_sensitive_identifier`). String literals and call expressions
//! are ignored.

use crate::diagnostic::{Diagnostic, Severity};

use super::helpers::is_sensitive_identifier;

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir {
        return;
    }
    let Some(op) = node.child_by_field_name("operator") else { return };
    let op_text = op.utf8_text(source).unwrap_or("");
    if op_text != "==" && op_text != "!=" && op_text != "===" && op_text != "!==" {
        return;
    }
    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    if is_literal_node(left) || is_literal_node(right) {
        return;
    }

    let left_name = operand_name(left, source);
    let right_name = operand_name(right, source);
    let left_hit = left_name.is_some_and(is_sensitive_identifier);
    let right_hit = right_name.is_some_and(is_sensitive_identifier);
    if !left_hit && !right_hit {
        return;
    }

    // Skip confirmation-style comparisons where both operands come from
    // the same object (e.g. `data.password === data.confirmPassword`).
    // These are form validation, not auth checks.
    if both_from_same_object(left, right, source) {
        return;
    }
    // Skip confirmation-pattern comparisons: both operands are sensitive
    // identifiers and one contains a confirmation prefix/suffix (e.g.
    // `password === confirmPassword`).
    if left_hit && right_hit
        && left.kind() == "identifier"
        && right.kind() == "identifier"
    {
        let l = left.utf8_text(source).unwrap_or("");
        let r = right.utf8_text(source).unwrap_or("");
        let combined = format!("{l}{r}");
        let lower = combined.to_ascii_lowercase();
        if lower.contains("confirm")
            || lower.contains("repeat")
            || lower.contains("retype")
            || lower.contains("verify")
        {
            return;
        }
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-timing-attack".into(),
        message: "Direct comparison of a security-sensitive value \u{2014} use a constant-time comparison (`crypto.timingSafeEqual`).".into(),
        severity: Severity::Error,
        span: None,
    });
}

fn is_literal_node(node: tree_sitter::Node) -> bool {
    matches!(
        node.kind(),
        "null" | "undefined" | "true" | "false" | "number" | "string"
    )
}

fn operand_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "identifier" => node.utf8_text(source).ok(),
        "member_expression" => node
            .child_by_field_name("property")
            .and_then(|p| p.utf8_text(source).ok()),
        _ => None,
    }
}

fn member_object_text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "member_expression" {
        return None;
    }
    node.child_by_field_name("object")
        .and_then(|o| o.utf8_text(source).ok())
}

fn both_from_same_object(left: tree_sitter::Node, right: tree_sitter::Node, source: &[u8]) -> bool {
    let left_obj = member_object_text(left, source);
    let right_obj = member_object_text(right, source);
    matches!((left_obj, right_obj), (Some(a), Some(b)) if a == b)
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_password_comparison() {
        assert_eq!(run_on("if (password === input) {}").len(), 1);
    }

    #[test]
    fn flags_auth_token_comparison() {
        assert_eq!(run_on("if (authToken == expectedAuthToken) {}").len(), 1);
    }

    /// `token` / `signature` without a secret indicator are non-security
    /// role words (lexer tokens, LSP signatures), not credentials.
    #[test]
    fn allows_comment_token_and_lsp_signature() {
        assert!(run_on("if (commentToken !== currentCommentToken) {}").is_empty());
        assert!(run_on("if (oldLspSig !== lspSignature) {}").is_empty());
    }

    #[test]
    fn flags_member_expression_password() {
        assert_eq!(run_on("if (user.password === input) {}").len(), 1);
    }

    #[test]
    fn flags_nested_member_expression_password() {
        assert_eq!(
            run_on("if (req.body.password === user.passwordHash) {}").len(),
            1
        );
    }

    #[test]
    fn flags_api_key_pascal_case() {
        assert_eq!(
            run_on("if (req.headers.apiKey === process.env.API_KEY) {}").len(),
            1
        );
    }

    #[test]
    fn allows_non_sensitive_comparison() {
        assert!(run_on("if (name === other) {}").is_empty());
    }

    #[test]
    fn allows_token_type_lexer() {
        assert!(run_on("if (tokenType === TokenType.Identifier) {}").is_empty());
    }

    #[test]
    fn allows_hash_map_size() {
        assert!(run_on("if (hashMapSize === 0) {}").is_empty());
    }

    #[test]
    fn allows_string_literal_with_sensitive_word() {
        // Comparison against a string literal containing a sensitive
        // word — `node.kind() !== "index_signature"` — is a call on the
        // left and a literal on the right. Neither is an inspected
        // operand kind.
        assert!(run_on(r#"if (node.kind() !== "index_signature") {}"#).is_empty());
    }

    #[test]
    fn allows_no_comparison() {
        assert!(run_on("const password = getPassword();").is_empty());
    }

    #[test]
    fn allows_null_check() {
        assert!(run_on("if (token === null) {}").is_empty());
    }

    #[test]
    fn allows_undefined_check() {
        assert!(run_on("if (password !== undefined) {}").is_empty());
    }

    #[test]
    fn allows_empty_string_check() {
        assert!(run_on(r#"if (secret === "") {}"#).is_empty());
    }

    #[test]
    fn allows_boolean_check() {
        assert!(run_on("if (token === false) {}").is_empty());
    }
}
