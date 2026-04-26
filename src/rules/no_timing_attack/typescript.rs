//! no-timing-attack TypeScript / JavaScript / TSX backend.
//!
//! Walks `binary_expression` nodes whose operator is `===` / `!==` /
//! `==` / `!=` and flags the comparison if either operand is an
//! identifier or member-access whose normalized last name ends with a
//! sensitive word (`password`, `token`, `signature`, `hash`, …).
//! String literals and call expressions are ignored.

use crate::diagnostic::{Diagnostic, Severity};

use super::helpers::is_sensitive_identifier;

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let Some(op) = node.child_by_field_name("operator") else { return };
    let op_text = op.utf8_text(source).unwrap_or("");
    if op_text != "==" && op_text != "!=" && op_text != "===" && op_text != "!==" {
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
        message: "Direct comparison of a security-sensitive value \u{2014} use a constant-time comparison (`crypto.timingSafeEqual`).".into(),
        severity: Severity::Error,
        span: None,
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_password_comparison() {
        assert_eq!(run_on("if (password === input) {}").len(), 1);
    }

    #[test]
    fn flags_user_token_comparison() {
        assert_eq!(run_on("if (userToken == expectedToken) {}").len(), 1);
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
}
