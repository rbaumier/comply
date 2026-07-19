//! OXC backend for consistent-empty-array-spread.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SpreadElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::SpreadElement(spread) = node.kind() else { return };

        // If the spread argument is a conditional (ternary), it's unparenthesized.
        // A parenthesized ternary would be wrapped in ParenthesizedExpression.
        if !matches!(spread.argument, Expression::ConditionalExpression(_)) {
            return;
        }

        // Only array-literal spread is precedence-ambiguous (`[...cond ? [a] : []]`).
        // Object-literal spread (`{...cond ? {a} : {}}`) is unambiguous, so skip it.
        if !matches!(
            semantic.nodes().parent_node(node.id()).kind(),
            AstKind::ArrayExpression(_)
        ) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, spread.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Parenthesize the ternary in array spread: \
                      `[...(condition ? ['a'] : [])]`.".into(),
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_rule;

    /// #6311: spreading an unparenthesized ternary into an *object* literal is
    /// unambiguous (`{...cond ? {} : {}}`) and must not fire.
    #[test]
    fn ignores_object_literal_spread_ternary() {
        let src = "const params = { ...config.sandbox ? { a: codeVerifier } : {} };";
        assert!(run_rule(&Check, src, "src/oauth.ts").is_empty());
    }

    /// Array-literal spread of an unparenthesized ternary is precedence-ambiguous
    /// and must still fire.
    #[test]
    fn flags_array_literal_spread_ternary() {
        let src = "const xs = [...cond ? ['a'] : []];";
        assert_eq!(run_rule(&Check, src, "src/arr.ts").len(), 1);
    }
}
