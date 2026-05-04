//! function-inside-loop oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_loop(kind: AstKind) -> bool {
    matches!(
        kind,
        AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_)
    )
}

fn is_function_boundary(kind: AstKind) -> bool {
    matches!(
        kind,
        AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::MethodDefinition(_)
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Function,
            AstType::ArrowFunctionExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Walk ancestors looking for a loop. Stop at function boundaries.
        let mut first = true;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            // Skip self.
            if first {
                first = false;
                continue;
            }
            let kind = ancestor.kind();
            if is_loop(kind) {
                let span = match node.kind() {
                    AstKind::Function(f) => f.span,
                    AstKind::ArrowFunctionExpression(a) => a.span,
                    _ => return,
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Function declared inside loop \u{2014} creates new function object each iteration.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }
            // Stop at enclosing function boundaries (not counting self).
            if is_function_boundary(kind) {
                return;
            }
        }
    }
}
