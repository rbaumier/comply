use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ForStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ForStatement(for_stmt) = node.kind() else {
            return;
        };
        let Some(test) = &for_stmt.test else {
            return;
        };
        if !contains_equality(test) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, for_stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`for` loop uses equality (`==`/`===`) in termination — use `<`, `<=`, `>`, or `>=` instead.".into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

/// Recursively check if an expression contains a numeric-counter equality
/// (`==` / `===`, but not `!=` / `!==`).
///
/// The rule's remediation (use `<`/`<=`/`>`/`>=`) only applies to a monotonic
/// numeric counter, where overshooting the equality target skips termination.
/// An equality is skipped when either operand is non-numeric — a string or
/// template literal (`token.type === "Punctuator"`), or a `.type`/`.kind`
/// member access (the string-discriminant convention used to walk linked
/// structures) — because no relational alternative exists for those.
fn contains_equality(expr: &Expression) -> bool {
    match expr {
        Expression::BinaryExpression(bin) => {
            (matches!(
                bin.operator,
                BinaryOperator::Equality | BinaryOperator::StrictEquality
            ) && !is_non_numeric_operand(&bin.left)
                && !is_non_numeric_operand(&bin.right))
                || contains_equality(&bin.left)
                || contains_equality(&bin.right)
        }
        Expression::LogicalExpression(log) => {
            contains_equality(&log.left) || contains_equality(&log.right)
        }
        _ => false,
    }
}

/// Whether an operand cannot be a monotonic numeric counter: a string or
/// template literal, or a `.type`/`.kind` member access (string discriminant).
fn is_non_numeric_operand(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => true,
        Expression::StaticMemberExpression(member) => {
            matches!(member.property.name.as_str(), "type" | "kind")
        }
        _ => false,
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
    fn flags_triple_equals() {
        assert_eq!(run_on("for (let i = 0; i === 10; i++) {}").len(), 1);
    }

    #[test]
    fn flags_double_equals() {
        assert_eq!(run_on("for (let i = 0; i == 10; i++) {}").len(), 1);
    }

    #[test]
    fn allows_less_than() {
        assert!(run_on("for (let i = 0; i < 10; i++) {}").is_empty());
    }

    #[test]
    fn allows_not_equals() {
        assert!(run_on("for (let i = 0; i !== 10; i++) {}").is_empty());
    }

    #[test]
    fn allows_string_literal_operand() {
        // Walks a token chain, stops when the string discriminant changes.
        // `token.type < "Punctuator"` is meaningless — no relational alternative.
        assert!(
            run_on(r#"for (let t = first; t.type === "Punctuator"; t = next(t)) {}"#).is_empty()
        );
    }

    #[test]
    fn allows_string_literal_operand_ast_chain() {
        // Walks the AST `alternate` chain, comparing a string discriminant.
        assert!(
            run_on(r#"for (let n = node; n.type === "IfStatement"; n = n.alternate) {}"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_string_literal_operand_compound_test() {
        // Verbatim eslint shape: the equality is one side of a `&&`.
        assert!(
            run_on(
                r#"for (let t = first; t.type === "Punctuator" && !isClosing(t); t = next(t)) {}"#
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_numeric_equality_in_compound_test() {
        // A numeric-counter equality inside a compound test must still flag.
        assert_eq!(
            run_on(r#"for (let i = 0; i === 10 && ok; i++) {}"#).len(),
            1
        );
    }

    #[test]
    fn allows_type_member_operand() {
        // `.type` member against a non-literal — string discriminant convention.
        assert!(run_on("for (let n = node; n.type === t; n = n.next) {}").is_empty());
    }

    #[test]
    fn allows_kind_member_operand() {
        // `.kind` member against a non-literal — string discriminant convention.
        assert!(run_on("for (let n = node; n.kind === k; n = n.next) {}").is_empty());
    }

    #[test]
    fn flags_two_plain_operands() {
        // Two non-string, non-discriminant operands — still a plausible counter.
        assert_eq!(run_on("for (let i = 0; i == n; i++) {}").len(), 1);
    }
}
