use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options",
];
const HOOK_METHODS: &[&str] = &[
    "onBeforeHandle",
    "onAfterHandle",
    "onError",
    "onRequest",
    "onTransform",
    "onParse",
    "onResponse",
];

pub struct Check;

/// Walk the chain from the outermost call inward, collecting method names
/// in call order `[foo, bar, baz]` for `app.foo(...).bar(...).baz(...)`.
fn chain_methods<'a>(call: &'a oxc_ast::ast::CallExpression<'a>) -> Vec<(&'a str, oxc_span::Span)> {
    let mut out = Vec::new();
    // Start: outermost call
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return out;
    };
    out.push((member.property.name.as_str(), call.span));

    let mut cur = &member.object;
    loop {
        let Expression::CallExpression(inner) = cur else { break };
        let Expression::StaticMemberExpression(m) = &inner.callee else { break };
        out.push((m.property.name.as_str(), inner.span));
        cur = &m.object;
    }
    out.reverse();
    out
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Only analyse the outermost call — skip if parent is a member expression.
        let parent = semantic.nodes().parent_node(node.id());
        if matches!(parent.kind(), AstKind::StaticMemberExpression(_) | AstKind::ComputedMemberExpression(_)) {
            return;
        }

        let methods = chain_methods(call);
        if methods.len() < 2 {
            return;
        }

        let mut seen_route = false;
        for (name, span) in &methods {
            if ROUTE_METHODS.contains(name) {
                seen_route = true;
                continue;
            }
            if seen_route && HOOK_METHODS.contains(name) {
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`.{}(...)` chained after route definitions — Elysia hooks only apply to routes registered after them.",
                        name
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_hook_after_route() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/', () => 'ok').onBeforeHandle(() => {});";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_onerror_after_post() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().post('/', () => 'ok').onError(() => {});";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_hook_before_route() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().onBeforeHandle(() => {}).get('/', () => 'ok');";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "new Elysia().get('/', () => 'ok').onBeforeHandle(() => {});";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
