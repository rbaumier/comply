//! no-await-expression-member OXC backend — flag member access on `(await expr)`.

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn unwrap_wrappers<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    let mut current = expr;
    loop {
        match current {
            Expression::ParenthesizedExpression(paren) => current = &paren.expression,
            Expression::TSNonNullExpression(ts) => current = &ts.expression,
            Expression::TSAsExpression(ts) => current = &ts.expression,
            Expression::TSSatisfiesExpression(ts) => current = &ts.expression,
            Expression::TSTypeAssertion(ts) => current = &ts.expression,
            _ => return current,
        }
    }
}

/// True when the member-access `node` sits in a conditionally-evaluated
/// (short-circuited) position: the right operand of a `??`/`||`/`&&`
/// `LogicalExpression`, or the `consequent`/`alternate` branch of a
/// `ConditionalExpression`. There the awaited call runs only on the taken branch,
/// so the rule's "extract to a variable" remediation would hoist the `await` to run
/// unconditionally — issuing the call (and, with a `...OrThrow`, throwing) even when
/// the short-circuit would have skipped it — changing runtime semantics. Span
/// containment against each operand/branch tells which side the access descends
/// through. The walk is bounded at the enclosing statement and at the first function
/// boundary: an `await` nested inside another function belongs to a different
/// execution scope, so an outer operator does not short-circuit it.
fn is_in_short_circuited_position(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let target = node.kind().span();
    let contains = |outer: oxc_span::Span| outer.start <= target.start && target.end <= outer.end;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::LogicalExpression(logical) => {
                if contains(logical.right.span()) {
                    return true;
                }
            }
            AstKind::ConditionalExpression(cond) => {
                if contains(cond.consequent.span()) || contains(cond.alternate.span()) {
                    return true;
                }
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            other if other.is_statement() => return false,
            _ => {}
        }
    }
    false
}

fn check_object_is_await(
    obj: &Expression,
    span_start: u32,
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let inner = unwrap_wrappers(obj);
    let Expression::AwaitExpression(await_expr) = inner else {
        return;
    };

    // `(await import(path)).default` is the canonical way to read a dynamic
    // module's exports — the namespace object only exists to be member-accessed,
    // so extracting it to a variable buys nothing.
    if matches!(unwrap_wrappers(&await_expr.argument), Expression::ImportExpression(_)) {
        return;
    }

    if is_in_short_circuited_position(node, semantic) {
        return;
    }

    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "Do not access a member directly from an await expression \
                  — extract to a variable first."
            .into(),
        severity: super::META.severity,
        span: None,
    });
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::StaticMemberExpression,
            AstType::ComputedMemberExpression,
        ]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["await"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::StaticMemberExpression(member) => {
                check_object_is_await(
                    &member.object,
                    member.span().start,
                    node,
                    semantic,
                    ctx,
                    diagnostics,
                );
            }
            AstKind::ComputedMemberExpression(member) => {
                check_object_is_await(
                    &member.object,
                    member.span().start,
                    node,
                    semantic,
                    ctx,
                    diagnostics,
                );
            }
            _ => {}
        }
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
    use crate::diagnostic::Severity;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_static_member_on_await() {
        let d = run("async function f() { (await fetch('/')).json(); }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Error);
    }

    #[test]
    fn flags_computed_member_on_await() {
        let d = run("async function f() { (await getItems())[0]; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_ts_non_null_assertion() {
        let d = run("async function f(p: Promise<{x:number}|null>) { (await p)!.x; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_ts_as_expression() {
        let d = run("async function f(p: Promise<unknown>) { (await p as {x:number}).x; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_ts_satisfies() {
        let d = run("async function f(p: Promise<unknown>) { (await p satisfies {x:number}).x; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_ts_angle_bracket_assertion() {
        let d = run("async function f(p: Promise<unknown>) { (<{x:number}>await p).x; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_nested_wrappers() {
        let d = run("async function f(p: Promise<unknown>) { ((await p)! as {x:number}).x; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_extracted_variable() {
        assert!(run("async function f() { const r = await fetch('/'); r.json(); }").is_empty());
    }

    #[test]
    fn allows_plain_await() {
        assert!(run("async function f() { await fetch('/'); }").is_empty());
    }

    #[test]
    fn allows_default_on_dynamic_import() {
        assert!(
            run("async function f() { const c = (await import('../../vitest.config.ts')).default; }")
                .is_empty()
        );
    }

    #[test]
    fn allows_named_member_on_dynamic_import() {
        assert!(run("async function f() { const x = (await import('./mod.ts')).foo; }").is_empty());
    }

    // Issue #1546: HTTP integration tests idiomatically access `.data`/`.status`
    // on an awaited response; the rule must not fire inside test files.
    #[test]
    fn skips_member_access_on_await_in_spec_file() {
        let d = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "async function f() { expect((await api.get('/admin/orders')).data.order.status).toBe('completed'); }",
            "integration-tests/http/__tests__/workflow-engine/admin/index.spec.ts",
        );
        assert!(d.is_empty(), "rule must be suppressed in spec files");
    }

    // Negative-space guard: the same smell in production (non-test) code still fires.
    #[test]
    fn still_flags_member_access_on_await_in_production_file() {
        let d = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "async function f() { return (await api.get('/admin/orders')).data; }",
            "src/services/order.ts",
        );
        assert_eq!(d.len(), 1, "rule must still fire in production code");
    }

    // Issue #7424: the awaited member access is the short-circuited operand of a
    // `??`/`||`/`&&` or a ternary branch; hoisting the await would run it
    // unconditionally, so the rule must not fire.
    #[test]
    fn allows_await_member_in_nullish_coalescing_right_operand() {
        assert!(
            run("async function f(a, b) { const r = a ?? (await b()).c; return r; }").is_empty()
        );
    }

    #[test]
    fn allows_await_member_in_logical_or_right_operand() {
        assert!(
            run("async function g(a, b) { const r = a || (await b()).c; return r; }").is_empty()
        );
    }

    #[test]
    fn allows_await_member_in_logical_and_right_operand() {
        assert!(
            run("async function h(a, b) { const r = a && (await b()).c; return r; }").is_empty()
        );
    }

    #[test]
    fn allows_await_member_in_ternary_consequent() {
        assert!(
            run("async function t(cond, b, d) { const r = cond ? (await b()).c : d; return r; }")
                .is_empty()
        );
    }

    #[test]
    fn allows_await_member_in_ternary_alternate() {
        assert!(
            run("async function u(cond, b, d) { const r = cond ? d : (await b()).c; return r; }")
                .is_empty()
        );
    }

    // Control: an unconditional statement-position access still fires — the await
    // always runs, so extracting it to a variable is a valid remediation.
    #[test]
    fn flags_await_member_in_unconditional_assignment() {
        let d = run("async function s(b) { const x = (await b()).c; return x; }");
        assert_eq!(d.len(), 1);
    }

    // Control: the LEFT operand of `??` is always evaluated, so it still fires.
    #[test]
    fn flags_await_member_in_nullish_coalescing_left_operand() {
        let d = run("async function l(a, b) { const r = (await b()).c ?? a; return r; }");
        assert_eq!(d.len(), 1);
    }
}
