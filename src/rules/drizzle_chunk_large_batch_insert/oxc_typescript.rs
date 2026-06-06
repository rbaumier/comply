//! drizzle-chunk-large-batch-insert OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

/// Walk leftward through a chained call to see if any receiver is a
/// `.insert(...)` call expression.
fn chain_has_insert(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(call) => {
            match &call.callee {
                Expression::StaticMemberExpression(member) => {
                    if member.property.name.as_str() == "insert" {
                        return true;
                    }
                    chain_has_insert(&member.object)
                }
                Expression::Identifier(id) => id.name.as_str() == "insert",
                _ => false,
            }
        }
        Expression::StaticMemberExpression(member) => chain_has_insert(&member.object),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["insert"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // X.values(...) — callee must be a static member with property `values`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "values" {
            return;
        }

        // The object side must resolve to a `.insert(...)` call.
        if !chain_has_insert(&member.object) {
            return;
        }

        // Single argument that is an array literal.
        if call.arguments.len() != 1 {
            return;
        }
        let Argument::ArrayExpression(arr) = &call.arguments[0] else {
            return;
        };

        let max = ctx.config.threshold("drizzle-chunk-large-batch-insert", "max", ctx.lang);
        let count = arr.elements.len();
        if count <= max {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, arr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Drizzle `.values([...])` with {count} rows exceeds the {max}-row chunking threshold — \
                 split into chunks to stay under the driver bind-parameter limit."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
