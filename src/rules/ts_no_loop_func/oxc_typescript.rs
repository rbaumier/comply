//! ts-no-loop-func OXC backend — flag function declarations/expressions and
//! arrow functions that appear inside loop bodies.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_loop_kind(kind: AstKind) -> bool {
    matches!(
        kind,
        AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_)
    )
}

fn is_function_boundary(kind: AstKind) -> bool {
    matches!(
        kind,
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
    )
}

fn is_inside_loop<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut first = true;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        // Skip the node itself.
        if first {
            first = false;
            continue;
        }
        let kind = ancestor.kind();
        // Stop at function boundaries — nested functions don't count.
        if is_function_boundary(kind) {
            return false;
        }
        if is_loop_kind(kind) {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_inside_loop(node, semantic) {
            return;
        }

        let span = match node.kind() {
            AstKind::Function(f) => f.span,
            AstKind::ArrowFunctionExpression(a) => a.span,
            _ => return,
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Function declared inside a loop — closures may \
                      capture the loop variable by reference. Move it outside."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
