//! no-nested-functions oxc backend — flag function declarations nested 3+ levels deep.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_function_kind(kind: AstKind) -> bool {
    matches!(
        kind,
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
    )
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
        // Count function ancestors (skip self)
        let mut depth = 0usize;
        let mut first = true;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if first {
                first = false;
                continue;
            }
            if is_function_kind(ancestor.kind()) {
                depth += 1;
            }
        }
        if depth < 2 {
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
            message: format!(
                "Function declared at nesting depth {} — extract to module scope.",
                depth
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
