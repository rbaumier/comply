//! promise-no-multiple-resolved oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, BindingPattern, Expression, FormalParameter, FunctionBody, Statement,
};
use std::sync::Arc;

pub struct Check;

/// Extract the identifier name of a formal parameter, if it's a simple
/// identifier binding (no destructuring).
fn param_identifier_name<'a>(param: &'a FormalParameter<'a>) -> Option<&'a str> {
    match &param.pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

/// Walk a function body counting top-level calls to `target_name` that
/// aren't guarded by an early-return (e.g. `if (x) { resolve(1); return; }`
/// is fine). The "top-level" approximation: only direct statements of the
/// function body — branches inside `if` blocks with a following `return`
/// don't count.
fn count_unguarded_calls<'a>(
    body: &'a FunctionBody<'a>,
    target: &str,
    out_spans: &mut Vec<u32>,
) {
    for stmt in body.statements.iter() {
        if let Statement::ExpressionStatement(es) = stmt
            && let Expression::CallExpression(call) = &es.expression
            && let Expression::Identifier(callee) = &call.callee
            && callee.name.as_str() == target
        {
            out_spans.push(call.span.start);
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["new Promise"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };
        let Expression::Identifier(ctor) = &new_expr.callee else { return };
        if ctor.name.as_str() != "Promise" {
            return;
        }
        let Some(arg) = new_expr.arguments.first() else { return };
        let (params, body) = match arg {
            Argument::ArrowFunctionExpression(a) => (&a.params, &a.body),
            Argument::FunctionExpression(f) => {
                let Some(b) = &f.body else { return };
                (&f.params, b)
            }
            _ => return,
        };
        let resolve_name = params.items.first().and_then(param_identifier_name);
        let reject_name = params.items.get(1).and_then(param_identifier_name);

        for (name, label) in [(resolve_name, "resolve"), (reject_name, "reject")] {
            let Some(n) = name else { continue };
            let mut spans = Vec::new();
            count_unguarded_calls(body, n, &mut spans);
            if spans.len() < 2 {
                continue;
            }
            // Flag every call past the first.
            for span_start in spans.iter().skip(1) {
                let (line, column) = byte_offset_to_line_col(ctx.source, *span_start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Multiple top-level `{label}(...)` calls in the same Promise \
                         executor — only the first settles the promise."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_double_resolve() {
        let src = "const p = new Promise((resolve) => { resolve(1); resolve(2); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_single_resolve() {
        let src = "const p = new Promise((resolve) => { resolve(1); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_resolve_in_branch() {
        let src = "const p = new Promise((resolve) => { if (x) resolve(1); else resolve(2); });";
        assert!(run(src).is_empty());
    }
}
