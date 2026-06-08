//! OxcCheck backend for no-misplaced-loop-counter — flag `for` loops
//! where the condition and update clause use different variables.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    AssignmentTarget, Expression, SimpleAssignmentTarget, UpdateExpression,
};
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

        let Some(test) = &for_stmt.test else { return };
        let Some(update) = &for_stmt.update else { return };

        let Some(cond_var) = extract_condition_var(test, ctx.source) else {
            return;
        };
        let Some(upd_var) = extract_update_var(update, ctx.source) else {
            return;
        };

        if cond_var != upd_var {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, for_stmt.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message:
                    "`for` loop condition and update use different variables — likely a copy-paste bug."
                        .into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

fn extract_condition_var<'a>(expr: &'a Expression<'a>, _source: &str) -> Option<&'a str> {
    match expr {
        Expression::BinaryExpression(bin) => {
            if let Expression::Identifier(id) = &bin.left {
                return Some(id.name.as_str());
            }
            None
        }
        _ => None,
    }
}

fn extract_update_var<'a>(expr: &'a Expression<'a>, source: &str) -> Option<&'a str> {
    match expr {
        Expression::UpdateExpression(upd) => extract_update_expr_var(upd),
        Expression::AssignmentExpression(assign) => match &assign.left {
            AssignmentTarget::AssignmentTargetIdentifier(id) => Some(id.name.as_str()),
            _ => None,
        },
        Expression::SequenceExpression(seq) => {
            seq.expressions.first().and_then(|e| extract_update_var(e, source))
        }
        _ => None,
    }
}

fn extract_update_expr_var<'a>(upd: &'a UpdateExpression<'a>) -> Option<&'a str> {
    match &upd.argument {
        SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => Some(id.name.as_str()),
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
    fn flags_different_vars() {
        assert_eq!(run_on("for (let i = 0; i < n; j++) {}").len(), 1);
    }

    #[test]
    fn flags_plus_equals_mismatch() {
        assert_eq!(run_on("for (let i = 0; i < n; j += 1) {}").len(), 1);
    }

    #[test]
    fn allows_matching_vars() {
        assert!(run_on("for (let i = 0; i < n; i++) {}").is_empty());
    }

    #[test]
    fn allows_matching_prefix() {
        assert!(run_on("for (let i = 0; i < 10; ++i) {}").is_empty());
    }
}
