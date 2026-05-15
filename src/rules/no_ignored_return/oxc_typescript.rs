//! no-ignored-return OXC backend — flag standalone calls to pure methods
//! whose return value is ignored.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;

pub struct Check;

const PURE_METHODS: &[&str] = &[
    "map",
    "filter",
    "slice",
    "concat",
    "trim",
    "replace",
    "toUpperCase",
    "toLowerCase",
    "split",
    "join",
];

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::ExpressionStatement(expr_stmt) = node.kind() else {
                continue;
            };
            let Expression::CallExpression(call) = &expr_stmt.expression else {
                continue;
            };
            let Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            let method_name = member.property.name.as_str();
            if !PURE_METHODS.contains(&method_name) {
                continue;
            }
            // Arrow concise body (`xs.map(fn)` is the implicit-return
            // expression of `() => xs.map(fn)`) wraps the call in an
            // ExpressionStatement under a FunctionBody, but the value
            // IS returned. Common JSX list pattern:
            // `{items.map(item => <Item />)}`
            let parent = semantic.nodes().parent_node(node.id());
            if let AstKind::FunctionBody(_) = parent.kind() {
                let grand = semantic.nodes().parent_node(parent.id());
                if let AstKind::ArrowFunctionExpression(arrow) = grand.kind()
                    && arrow.expression
                {
                    continue;
                }
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, expr_stmt.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Return value of `.{}` is ignored — the call has no side effect.",
                    method_name
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_standalone_map_call() {
        let src = "function f(xs) { xs.map(x => x); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_arrow_concise_body_returning_map() {
        // Regression for rbaumier/comply#20 — `.map(...)` returning JSX
        // child as the implicit return of an arrow.
        let src = "const f = xs => xs.map(x => x);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_assigned_map_call() {
        let src = "const result = xs.map(x => x);";
        assert!(run(src).is_empty());
    }
}
