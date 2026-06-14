use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryExpression, Expression};
use std::sync::Arc;

use super::helpers::is_sensitive_identifier;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        let AstKind::BinaryExpression(bin) = node.kind() else {
            return;
        };
        let op = bin.operator.as_str();
        if op != "==" && op != "!=" && op != "===" && op != "!==" {
            return;
        }
        if is_literal_expr(&bin.left) || is_literal_expr(&bin.right) {
            return;
        }
        let left_name = operand_name(&bin.left);
        let right_name = operand_name(&bin.right);
        let left_hit = left_name.as_deref().is_some_and(is_sensitive_identifier);
        let right_hit = right_name.as_deref().is_some_and(is_sensitive_identifier);
        if !left_hit && !right_hit {
            return;
        }
        // Skip confirmation-style comparisons where both operands come from
        // the same object (e.g. `data.password === data.confirmPassword`).
        if both_from_same_object(bin) {
            return;
        }
        // Skip confirmation-pattern comparisons: both operands are sensitive
        // identifiers and one contains a confirmation prefix/suffix.
        if left_hit && right_hit && is_identifier(&bin.left) && is_identifier(&bin.right)
            && let (Some(l), Some(r)) = (&left_name, &right_name) {
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
        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Direct comparison of a security-sensitive value \u{2014} use a constant-time comparison (`crypto.timingSafeEqual`).".into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

fn is_literal_expr(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::NullLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::StringLiteral(_)
    ) || is_undefined(expr)
}

fn is_undefined(expr: &Expression) -> bool {
    matches!(expr, Expression::Identifier(id) if id.name == "undefined")
}

fn is_identifier(expr: &Expression) -> bool {
    matches!(expr, Expression::Identifier(_))
}

fn operand_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::StaticMemberExpression(member) => Some(member.property.name.to_string()),
        _ => None,
    }
}

fn member_object_text(expr: &Expression) -> Option<String> {
    match expr {
        Expression::StaticMemberExpression(member) => {
            expr_text(&member.object)
        }
        _ => None,
    }
}

fn expr_text(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::StaticMemberExpression(member) => {
            let obj = expr_text(&member.object)?;
            Some(format!("{}.{}", obj, member.property.name))
        }
        _ => None,
    }
}

fn both_from_same_object(bin: &BinaryExpression) -> bool {
    let left_obj = member_object_text(&bin.left);
    let right_obj = member_object_text(&bin.right);
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
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

    /// `token` / `signature` without a secret indicator are non-security
    /// role words (lexer tokens, LSP signatures), not credentials.
    #[test]
    fn allows_comment_token_and_lsp_signature() {
        assert!(run_on("if (commentToken !== currentCommentToken) {}").is_empty());
        assert!(run_on("if (oldLspSig !== lspSignature) {}").is_empty());
    }

    #[test]
    fn allows_string_literal_with_sensitive_word() {
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
