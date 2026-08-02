//! non-existent-operator oxc backend — detect typo operators `=+`, `=-`, `=!`.
//! Spacing decides — see [`super::reads_as_compact_assignment`].

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentOperator, Expression, UnaryOperator};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AssignmentExpression(assign) = node.kind() else { return };

        // Must be a plain `=` assignment (not `+=`, `-=`, etc.)
        if assign.operator != AssignmentOperator::Assign {
            return;
        }

        // RHS must be a unary expression with +, -, or !
        let Expression::UnaryExpression(unary) = &assign.right else { return };
        if !matches!(
            unary.operator,
            UnaryOperator::UnaryPlus | UnaryOperator::UnaryNegation | UnaryOperator::LogicalNot
        ) {
            return;
        }

        // The unary expression starts at its sign, so the byte before it is the
        // `=` when the two are glued. `x = +1` keeps them apart: a real unary.
        let sign_offset = unary.span.start as usize;
        let Some(eq_offset) = sign_offset.checked_sub(1) else { return };
        if ctx.source.as_bytes().get(eq_offset) != Some(&b'=') {
            return;
        }

        // `x=-1` and `flag=!0` are compact assignments of a signed value.
        if super::reads_as_compact_assignment(ctx.source, eq_offset) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Typo operator — did you mean `+=`, `-=`, or `!=`?".into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.js")
    }

    #[test]
    fn flags_typo_operators() {
        assert_eq!(run_on("let c = 0; c =+ 1;").len(), 1);
        assert_eq!(run_on("let c = 0; c =- 1;").len(), 1);
        assert_eq!(run_on("let c = 0; c=- 1;").len(), 1);
        assert_eq!(run_on("function f(a, b) { return (a =! b); }").len(), 1);
    }

    /// A space on one side of the `=`/sign pair is enough to make their contact
    /// meaningful: no minifier and no formatter writes an assignment that way.
    #[test]
    fn flags_half_spaced_typo_operators() {
        assert_eq!(run_on("let c = 0; c =-1;").len(), 1);
        assert_eq!(run_on("let f = false; f =!0;").len(), 1);
    }

    #[test]
    fn allows_spaced_unary_sign() {
        assert!(run_on("let c = 0; c = -1;").is_empty());
        assert!(run_on("let c = 0; c = +1;").is_empty());
    }

    /// Minifiers spell `true` / `false` as `!0` / `!1`, and drop the spaces
    /// around `=`. The sign belongs to the constant, not to the operator.
    #[test]
    fn allows_minified_boolean_constants() {
        assert!(run_on("let f = false; f=!0;").is_empty());
        assert!(run_on("const s = {}; s.ready=!1;").is_empty());
        assert!(run_on("const s = {}; s.on=!!s.raw;").is_empty());
    }

    /// Compact spacing glues every token to the next one, so the sign touching
    /// the `=` says nothing: it stays a unary sign on the value.
    #[test]
    fn allows_compact_unary_sign() {
        assert!(run_on("let c = 0; c=-1;").is_empty());
        assert!(run_on("let n = 0; n=+document.title;").is_empty());
        assert!(run_on("let c = 0; c=-(1 + 2);").is_empty());
    }
}
