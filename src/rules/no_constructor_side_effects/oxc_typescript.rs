//! OXC backend for no-constructor-side-effects.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        // Only flag when the `new` expression is the direct child of an ExpressionStatement
        // (i.e. used as a statement, not assigned/returned/thrown).
        let parent = semantic.nodes().parent_node(node.id());
        if !matches!(parent.kind(), AstKind::ExpressionStatement(_)) {
            return;
        }

        // Arrow function with concise-expression body (`() => new Set(value)`)
        // wraps the expression in an ExpressionStatement under a FunctionBody,
        // but the value IS returned — not a side-effect call. Common in
        // useMemo / useCallback / useRef lazy-init callbacks.
        let grandparent = semantic.nodes().parent_node(parent.id());
        if let AstKind::FunctionBody(_) = grandparent.kind() {
            let great = semantic.nodes().parent_node(grandparent.id());
            if let AstKind::ArrowFunctionExpression(arrow) = great.kind()
                && arrow.expression
            {
                return;
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new X()` without assignment — constructors should not be called for side effects.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_unassigned_new_statement() {
        let src = "function f() { new MyClass(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_new_returned_from_arrow_expression() {
        // Regression for rbaumier/comply#20 — useMemo lazy init.
        let src = r#"const s = useMemo(() => new Set(value), [value]);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_new_assigned() {
        let src = "const m = new Map();";
        assert!(run(src).is_empty());
    }
}
