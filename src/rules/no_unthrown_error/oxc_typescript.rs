use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };
        let Expression::Identifier(id) = &new_expr.callee else {
            return;
        };
        if id.name.as_str() != "Error" {
            return;
        }
        let parent = semantic.nodes().parent_node(node.id());
        if !matches!(parent.kind(), AstKind::ExpressionStatement(_)) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new Error(...)` is created but never thrown — add `throw` or assign the error.".into(),
            severity: super::META.severity,
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
    fn flags_unthrown_error() {
        assert_eq!(run_on("  new Error(\"oops\");").len(), 1);
    }

    #[test]
    fn flags_bare_new_error() {
        assert_eq!(run_on("new Error(\"something went wrong\");").len(), 1);
    }

    #[test]
    fn allows_thrown_error() {
        assert!(run_on("throw new Error(\"oops\");").is_empty());
    }

    #[test]
    fn allows_assigned_error() {
        assert!(run_on("const err = new Error(\"oops\");").is_empty());
    }

    #[test]
    fn allows_returned_error() {
        assert!(run_on("function f() { return new Error(\"oops\"); }").is_empty());
    }
}
