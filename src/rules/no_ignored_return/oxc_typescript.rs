//! no-ignored-return OXC backend — flag standalone calls to pure methods
//! whose return value is ignored.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExpressionStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ExpressionStatement(expr_stmt) = node.kind() else {
            return;
        };
        let Expression::CallExpression(call) = &expr_stmt.expression else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method_name = member.property.name.as_str();
        if !PURE_METHODS.contains(&method_name) {
            return;
        }
        // `String.prototype.replace`/`replaceAll` is only pure with a string
        // replacement. With a function replacer (`replace(re, (...m) => {...})`)
        // the callback carries the side effects and the discarded string
        // result is the canonical "iterate every match" idiom — not dead.
        if matches!(method_name, "replace" | "replaceAll")
            && let Some(
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_),
            ) = call.arguments.get(1).and_then(|arg| arg.as_expression())
        {
            return;
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
                return;
            }
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, expr_stmt.span.start as usize);
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
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

    #[test]
    fn allows_replace_with_arrow_replacer() {
        // Regression for rbaumier/comply#3963 — `String.prototype.replace`
        // used as a side-effecting match iterator: the discarded return
        // value is legitimate because the replacer callback does the work.
        let src = "function f(source, re) { source.replace(re, (...m) => { push(m); return ''; }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_replace_all_with_function_replacer() {
        // Regression for rbaumier/comply#3963 — function-expression replacer.
        let src = "function f(s, re) { s.replaceAll(re, function (m) { side(m); return ''; }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_replace_with_string_replacement() {
        // A string replacement is genuinely pure — the discarded return
        // value is dead, so the call must still flag.
        let src = "function f(source, re) { source.replace(re, 'x'); }";
        assert_eq!(run(src).len(), 1);
    }
}
