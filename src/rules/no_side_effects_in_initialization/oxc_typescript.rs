//! no-side-effects-in-initialization OxcCheck backend — flag module-level
//! expression statements whose expression is a call or `new` expression.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    [".test.", ".test-d.", ".spec.", "__tests__", "_test.", ".e2e."]
        .iter()
        .any(|m| s.contains(m))
}

fn effectful_expression_label(expr: &Expression) -> Option<&'static str> {
    match expr {
        Expression::CallExpression(_) => Some("call"),
        Expression::NewExpression(_) => Some("`new` expression"),
        _ => None,
    }
}

fn has_pure_annotation(source: &str, span_start: usize) -> bool {
    // Look backwards from the statement start for a PURE comment.
    let before = &source[..span_start];
    let trimmed = before.trim_end();
    trimmed.ends_with("*/")
        && (trimmed.contains("#__PURE__") || trimmed.contains("@__PURE__"))
        && {
            // The comment must be the immediately preceding token.
            if let Some(comment_start) = trimmed.rfind("/*") {
                let comment = &trimmed[comment_start..];
                comment.contains("#__PURE__") || comment.contains("@__PURE__")
            } else {
                false
            }
        }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir || is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for stmt in &semantic.nodes().program().body {
            let Statement::ExpressionStatement(expr_stmt) = stmt else { continue };
            let Some(label) = effectful_expression_label(&expr_stmt.expression) else {
                continue;
            };

            if has_pure_annotation(ctx.source, expr_stmt.span.start as usize) {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, expr_stmt.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Top-level {label} executes on import and blocks tree-shaking. \
                     Move it into a function, or mark it `/*#__PURE__*/` if truly side-effect-free."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
