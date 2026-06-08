//! no-nested-ternary — OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ConditionalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ConditionalExpression(cond) = node.kind() else { return };

        let parent = semantic.nodes().parent_node(node.id());
        if !matches!(parent.kind(), AstKind::ConditionalExpression(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, cond.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Nested ternary — extract to if/else or a named variable for each branch."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_nested_ternary() {
        let diags = run_on("const x = a ? b ? 1 : 2 : 3;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_single_ternary() {
        assert!(run_on("const x = a ? 1 : 2;").is_empty());
    }


    #[test]
    fn flags_deeply_nested_ternaries() {
        assert_eq!(run_on("const x = a ? b ? c ? 1 : 2 : 3 : 4;").len(), 2);
    }
}
