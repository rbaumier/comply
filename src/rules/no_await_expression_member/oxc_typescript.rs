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

fn check_object_is_await(
    obj: &Expression,
    span_start: u32,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let inner = unwrap_wrappers(obj);
    if !matches!(inner, Expression::AwaitExpression(_)) {
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
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::StaticMemberExpression(member) => {
                check_object_is_await(&member.object, member.span().start, ctx, diagnostics);
            }
            AstKind::ComputedMemberExpression(member) => {
                check_object_is_await(&member.object, member.span().start, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
}
