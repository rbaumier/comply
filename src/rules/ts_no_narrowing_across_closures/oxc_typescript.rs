//! ts-no-narrowing-across-closures oxc backend.
//!
//! Inside an `if (x)` / `if (x === null)` / `if (x !== undefined)` block whose
//! test narrows a reassignable binding `x` (a `let`/`var`/parameter that is
//! written somewhere — never a `const` or a never-reassigned binding, whose
//! narrowing TypeScript preserves), if a call to
//! `setTimeout`/`.then`/`.catch`/`addEventListener` takes a function expression
//! that references `x` directly (not captured by a local const), flag it.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BinaryOperator, BindingPattern, Expression, IdentifierReference, Statement, UnaryOperator,
};
use oxc_semantic::{ReferenceFlags, Semantic, SymbolId};
use oxc_span::GetSpan;
use std::sync::Arc;

const CLOSURE_CALLEES: &[&str] = &[
    "setTimeout",
    "setInterval",
    "queueMicrotask",
    "requestAnimationFrame",
];
const CLOSURE_METHODS: &[&str] = &["then", "catch", "finally", "addEventListener"];

pub struct Check;

/// `null`, `undefined`, or `void 0` — the right-hand sides that make a `===`/`!==`
/// comparison an actual nullability narrowing of the left operand.
fn is_nullish_literal(expr: &Expression) -> bool {
    match expr {
        Expression::NullLiteral(_) => true,
        Expression::Identifier(id) => id.name.as_str() == "undefined",
        Expression::UnaryExpression(unary) => unary.operator == UnaryOperator::Void,
        _ => false,
    }
}

fn narrowed_identifier<'a>(expr: &'a Expression<'a>) -> Option<&'a IdentifierReference<'a>> {
    match expr {
        Expression::Identifier(id) => Some(id),
        Expression::BinaryExpression(bin) => {
            // Only equality comparisons against a nullish literal narrow the left
            // operand. `x !== unrelated` / `x === other` change no nullability.
            let is_equality = matches!(
                bin.operator,
                BinaryOperator::StrictEquality
                    | BinaryOperator::StrictInequality
                    | BinaryOperator::Equality
                    | BinaryOperator::Inequality
            );
            if is_equality
                && is_nullish_literal(&bin.right)
                && let Expression::Identifier(id) = &bin.left
            {
                Some(id)
            } else {
                None
            }
        }
        Expression::ParenthesizedExpression(paren) => narrowed_identifier(&paren.expression),
        _ => None,
    }
}

fn is_closure_call(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => CLOSURE_CALLEES.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            CLOSURE_METHODS.contains(&member.property.name.as_str())
        }
        _ => false,
    }
}

fn has_local_const(stmts: &oxc_allocator::Vec<'_, Statement<'_>>, name: &str) -> bool {
    for stmt in stmts.iter() {
        if let Statement::VariableDeclaration(vd) = stmt {
            if !vd.kind.is_const() {
                continue;
            }
            for decl in &vd.declarations {
                if let BindingPattern::BindingIdentifier(id) = &decl.id
                    && id.name.as_str() == name {
                        return true;
                    }
            }
        }
    }
    false
}

fn span_contains(outer: oxc_span::Span, inner: oxc_span::Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// True when the binding has a write reference, i.e. it is reassigned somewhere.
/// A `const` and a never-reassigned `let`/`var`/parameter both have none. Modern
/// TypeScript preserves the narrowing of a never-reassigned binding inside nested
/// closures, so only a reassignable binding can lose narrowing across a closure.
fn is_reassigned(semantic: &Semantic<'_>, symbol: SymbolId) -> bool {
    semantic
        .scoping()
        .get_resolved_references(symbol)
        .any(|reference| reference.flags().contains(ReferenceFlags::Write))
}

/// Returns true when an actual reference to `symbol` lies within `body_span`.
///
/// Enumerates the binding's resolved references and tests each reference node's
/// span for containment. Object-literal property keys and member-expression
/// property names are not `IdentifierReference` nodes, so they never appear here.
fn callback_body_references(
    semantic: &Semantic<'_>,
    body_span: oxc_span::Span,
    symbol: SymbolId,
) -> bool {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    scoping.get_resolved_references(symbol).any(|reference| {
        let ref_span = nodes.kind(reference.node_id()).span();
        span_contains(body_span, ref_span)
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::IfStatement(stmt) = node.kind() else {
            return;
        };
        let Some(ident) = narrowed_identifier(&stmt.test) else {
            return;
        };
        let name = ident.name.as_str();
        // Resolve the narrowed binding to a symbol; an unresolvable reference
        // (global, ambient) cannot be checked semantically, so do not flag.
        let Some(symbol) = ident
            .reference_id
            .get()
            .and_then(|ref_id| semantic.scoping().get_reference(ref_id).symbol_id())
        else {
            return;
        };
        // A const or never-reassigned binding keeps its narrowing inside closures,
        // so scheduling a closure that uses it loses nothing.
        if !is_reassigned(semantic, symbol) {
            return;
        }
        let Statement::BlockStatement(block) = &stmt.consequent else {
            return;
        };
        if has_local_const(&block.body, name) {
            return;
        }

        // Walk the block body looking for closure-scheduling calls.
        visit_stmts(&block.body, name, symbol, semantic, ctx, diagnostics);
    }
}

fn visit_stmts(
    stmts: &oxc_allocator::Vec<'_, Statement<'_>>,
    name: &str,
    symbol: SymbolId,
    semantic: &Semantic<'_>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in stmts.iter() {
        visit_stmt(stmt, name, symbol, semantic, ctx, diagnostics);
    }
}

fn visit_stmt(
    stmt: &Statement,
    name: &str,
    symbol: SymbolId,
    semantic: &Semantic<'_>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        Statement::ExpressionStatement(es) => {
            visit_expr(&es.expression, name, symbol, semantic, ctx, diagnostics);
        }
        Statement::BlockStatement(block) => {
            visit_stmts(&block.body, name, symbol, semantic, ctx, diagnostics);
        }
        Statement::IfStatement(ifs) => {
            visit_stmt(&ifs.consequent, name, symbol, semantic, ctx, diagnostics);
            if let Some(alt) = &ifs.alternate {
                visit_stmt(alt, name, symbol, semantic, ctx, diagnostics);
            }
        }
        Statement::VariableDeclaration(vd) => {
            for decl in &vd.declarations {
                if let Some(init) = &decl.init {
                    visit_expr(init, name, symbol, semantic, ctx, diagnostics);
                }
            }
        }
        _ => {}
    }
}

fn visit_expr(
    expr: &Expression,
    name: &str,
    symbol: SymbolId,
    semantic: &Semantic<'_>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Expression::CallExpression(call) = expr else {
        return;
    };
    if !is_closure_call(&call.callee) {
        return;
    }
    for arg in &call.arguments {
        let Some(arg_expr) = arg.as_expression() else {
            continue;
        };
        let (is_callback, body_span, cb_span) = match arg_expr {
            Expression::ArrowFunctionExpression(arrow) => {
                (true, arrow.body.span, arrow.span)
            }
            Expression::FunctionExpression(func) => {
                if let Some(body) = &func.body {
                    (true, body.span, func.span())
                } else {
                    continue;
                }
            }
            _ => continue,
        };
        if !is_callback {
            continue;
        }
        if callback_body_references(semantic, body_span, symbol) {
            let (line, column) = byte_offset_to_line_col(ctx.source, cb_span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Variable `{name}` loses its narrowing inside this callback; capture it in a local const first."
                ),
                severity: Severity::Warning,
                span: None,
            });
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_set_timeout_using_narrowed_var() {
        let src = "function f() { let user: { name: string } | null = get(); if (user) { setTimeout(() => console.log(user.name), 0); } user = null; }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_promise_then() {
        let src = "function f() { let u: string | null = get(); if (u) { p.then(() => console.log(u)); } u = null; }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_local_const_capture() {
        let src = "function f() { let user: { name: string } | null = get(); if (user) { const user = user; setTimeout(() => console.log(user.name), 0); } user = null; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_plain_usage_without_closure() {
        let src =
            "function f() { let user: { name: string } | null = get(); if (user) { console.log(user.name); } user = null; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_object_literal_key_matching_name() {
        let src = "function f(error: Err | null, promise: Promise<void>, opts: any, path: string[]) { if (error) { promise.catch((cause) => { opts.onError?.({ error: cause, path }); }); promise = Promise.reject(error); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_member_property_name_matching_name() {
        let src = "function g(u: string | null, p: Promise<{ u: string }>, use: (x: string) => void) { if (u) { p.then((res) => { console.log(res.u); }); use(u); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_real_reference_inside_callback() {
        let src = "function f(promise: Promise<void>) { let error: Err | null = get(); if (error) { promise.catch(() => { console.log(error); }); } error = null; }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_const_binding_narrowed_then_used_in_closure() {
        // A const keeps its narrowing across closures, so nothing is lost.
        let src = "function f() { const x = get(); if (x) { setTimeout(() => x.foo()); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_const_binding_with_non_nullability_test() {
        // const promise + `!==` against an unrelated value: neither the binding
        // nor the operator can lose narrowing. (jotai unwrap.ts repro.)
        let src = "function f(prev) { const promise = get(); if (!isPromiseLike(promise)) return; if (promise !== prev?.p) { promise.then((v) => cache.set(promise, v)); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_reassigned_let_with_non_nullability_test() {
        // Even a reassignable binding is not narrowed by `!==` against an
        // unrelated value, so the closure loses no narrowing.
        let src = "function f(prev) { let promise = get(); if (promise !== prev?.p) { promise.then((v) => cache.set(promise, v)); } promise = get(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_reassigned_let_narrowed_truthy() {
        // Reassigned `let` (write reference) narrowed by a truthy test and used
        // in a closure: narrowing is genuinely lost across the closure.
        let src = "function f() { let x = get(); if (x) { setTimeout(() => x.foo()); } x = null; }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_reassigned_let_narrowed_nullability() {
        let src = "function f() { let x = get(); if (x !== null) { setTimeout(() => x.foo()); } x = null; }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }
}
