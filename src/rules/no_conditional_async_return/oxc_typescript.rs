//! OxcCheck backend for no-conditional-async-return.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, LogicalOperator, Statement, TSType, TSTypeAnnotation, TSTypeName};
use std::sync::Arc;

pub struct Check;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReturnKind {
    Sync,
    Promise,
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::Function(func) => {
                    if func.r#async {
                        continue;
                    }
                    let Some(body) = &func.body else {
                        continue;
                    };
                    if annotation_permits_promise_branches(func.return_type.as_deref()) {
                        continue;
                    }
                    let kinds = collect_return_kinds(&body.statements, ctx.source);
                    if kinds.contains(&ReturnKind::Sync) && kinds.contains(&ReturnKind::Promise) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, func.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Function mixes sync and promise-returning branches — unify to `Promise<T>` (async) or plain `T` everywhere.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    if arrow.r#async {
                        continue;
                    }
                    if arrow.expression {
                        continue;
                    }
                    if annotation_permits_promise_branches(arrow.return_type.as_deref()) {
                        continue;
                    }
                    let kinds = collect_return_kinds(&arrow.body.statements, ctx.source);
                    if kinds.contains(&ReturnKind::Sync) && kinds.contains(&ReturnKind::Promise) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Function mixes sync and promise-returning branches — unify to `Promise<T>` (async) or plain `T` everywhere.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

/// Is this type a `Promise<...>` reference?
fn is_promise_type(ty: &TSType) -> bool {
    if let TSType::TSTypeReference(tref) = ty
        && let TSTypeName::IdentifierReference(id) = &tref.type_name
    {
        return id.name.as_str() == "Promise";
    }
    false
}

/// Does the explicit return-type annotation document an all-promise or
/// mixed-promise contract? A plain `Promise<T>` guarantees every branch is a
/// Promise (TypeScript enforces assignability), and a `Promise<T>`-bearing
/// union (e.g. `void | Promise<void>`, `T | Promise<T>`) deliberately documents
/// the mixed-return contract. In both cases the conditional return is
/// intentional, so trust the annotation rather than the syntactic classifier.
fn annotation_permits_promise_branches(return_type: Option<&TSTypeAnnotation>) -> bool {
    let Some(rt) = return_type else { return false };
    let mut ty = &rt.type_annotation;
    while let TSType::TSParenthesizedType(paren) = ty {
        ty = &paren.type_annotation;
    }
    if is_promise_type(ty) {
        return true;
    }
    let TSType::TSUnionType(union) = ty else {
        return false;
    };
    union.types.iter().any(is_promise_type) && union.types.iter().any(|t| !is_promise_type(t))
}

/// Classify a return-value expression as promise-returning or sync.
fn classify_value(expr: &Expression, source: &str) -> ReturnKind {
    // `a ?? Promise.resolve()` / `a || Promise.resolve()`: a single
    // Promise-typed operand makes the whole expression resolve to a Promise.
    if let Expression::LogicalExpression(logical) = expr
        && matches!(logical.operator, LogicalOperator::Coalesce | LogicalOperator::Or)
        && (classify_value(&logical.left, source) == ReturnKind::Promise
            || classify_value(&logical.right, source) == ReturnKind::Promise)
    {
        return ReturnKind::Promise;
    }

    let Expression::CallExpression(call) = expr else {
        return ReturnKind::Sync;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return ReturnKind::Sync;
    };
    let method = member.property.name.as_str();

    // `.then(...)` / `.catch(...)` / `.finally(...)` on any receiver
    if method == "then" || method == "catch" || method == "finally" {
        return ReturnKind::Promise;
    }

    // `Promise.<combinator>(...)`
    if let Expression::Identifier(obj) = &member.object
        && obj.name.as_str() == "Promise"
            && matches!(
                method,
                "resolve" | "reject" | "all" | "allSettled" | "race" | "any"
            )
        {
            return ReturnKind::Promise;
        }

    ReturnKind::Sync
}

/// Walk statements collecting return kinds. Skip nested function bodies.
fn collect_return_kinds(stmts: &[Statement], source: &str) -> Vec<ReturnKind> {
    let mut out = Vec::new();
    for stmt in stmts {
        collect_from_stmt(stmt, source, &mut out);
    }
    out
}

fn collect_from_stmt(stmt: &Statement, source: &str, out: &mut Vec<ReturnKind>) {
    match stmt {
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                out.push(classify_value(arg, source));
            }
        }
        // Don't descend into nested functions
        Statement::FunctionDeclaration(_) => {}
        Statement::BlockStatement(block) => {
            for s in &block.body {
                collect_from_stmt(s, source, out);
            }
        }
        Statement::IfStatement(if_stmt) => {
            collect_from_stmt(&if_stmt.consequent, source, out);
            if let Some(alt) = &if_stmt.alternate {
                collect_from_stmt(alt, source, out);
            }
        }
        Statement::SwitchStatement(switch) => {
            for case in &switch.cases {
                for s in &case.consequent {
                    collect_from_stmt(s, source, out);
                }
            }
        }
        Statement::TryStatement(try_stmt) => {
            for s in &try_stmt.block.body {
                collect_from_stmt(s, source, out);
            }
            if let Some(handler) = &try_stmt.handler {
                for s in &handler.body.body {
                    collect_from_stmt(s, source, out);
                }
            }
            if let Some(finalizer) = &try_stmt.finalizer {
                for s in &finalizer.body {
                    collect_from_stmt(s, source, out);
                }
            }
        }
        Statement::ForStatement(for_stmt) => {
            collect_from_stmt(&for_stmt.body, source, out);
        }
        Statement::ForInStatement(for_in) => {
            collect_from_stmt(&for_in.body, source, out);
        }
        Statement::ForOfStatement(for_of) => {
            collect_from_stmt(&for_of.body, source, out);
        }
        Statement::WhileStatement(while_stmt) => {
            collect_from_stmt(&while_stmt.body, source, out);
        }
        Statement::DoWhileStatement(do_while) => {
            collect_from_stmt(&do_while.body, source, out);
        }
        Statement::LabeledStatement(labeled) => {
            collect_from_stmt(&labeled.body, source, out);
        }
        // ExpressionStatement containing arrow/function — skip (nested fn)
        Statement::ExpressionStatement(es) => {
            if matches!(
                es.expression,
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
            ) {
                // nested function — don't descend
            }
        }
        _ => {}
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
    fn flags_mixed_without_explicit_union_annotation() {
        // Negative-space guard: a genuinely inconsistent async function with no
        // explicit `T | Promise<T>` return type still fires.
        let src = "function f(x: boolean) { if (x) return 1; return Promise.resolve(2); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_explicit_void_promise_union_return_type() {
        // Regression for #1619: Astro's `renderChild` declares its mixed-return
        // contract via `void | Promise<void>`, so the fast sync path is
        // intentional, not a mistake.
        let src = "function renderChild(destination: D, child: any): void | Promise<void> {
            if (typeof child === 'string') { destination.write(child); return; }
            if (isPromise(child)) { return child.then((x) => renderChild(destination, x)); }
            destination.write(child);
            return;
        }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_explicit_generic_union_return_type() {
        let src = "function f<T>(x: boolean, v: T): T | Promise<T> { if (x) return v; return Promise.resolve(v); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_explicit_union_return_type_on_arrow() {
        let src = "const f = (x: boolean): void | Promise<void> => { if (x) { return; } return Promise.resolve(); };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_mixed_when_annotation_is_not_a_promise_union() {
        // A non-union (or non-Promise-bearing) annotation does not document a
        // mixed-return contract, so the inconsistency still fires.
        let src = "function f(x: boolean): number { if (x) return 1; return Promise.resolve(2); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_plain_promise_annotation_with_coalesce_branch() {
        // Regression for #3858: TanStack/query `runNext` is annotated
        // `Promise<unknown>`, so every branch is contractually a Promise even
        // though `foundMutation?.continue() ?? Promise.resolve()` is not
        // syntactically recognized as one.
        let src = "function runNext(s: string): Promise<unknown> {
            if (typeof s === 'string') {
                return foundMutation?.continue() ?? Promise.resolve();
            } else {
                return Promise.resolve();
            }
        }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_plain_promise_annotation_with_method_call_branch() {
        // Regression for #3858: `ensureQueryData` returns a method call
        // (`this.fetchQuery(options)`) in one branch and `Promise.resolve` in
        // another, both contractually `Promise<TData>`.
        let src = "function ensureQueryData(options: any): Promise<TData> {
            if (cachedData === undefined) {
                return this.fetchQuery(options);
            }
            return Promise.resolve(cachedData);
        }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unannotated_arrow_with_coalesce_promise_branch() {
        // Regression for #3858 (part 2): an un-annotated arrow whose branches
        // are `x ?? Promise.resolve()` and `Promise.resolve()` is all-Promise.
        let src = "const f = (x: boolean) => { if (x) { return foo() ?? Promise.resolve(); } return Promise.resolve(); };";
        assert!(run(src).is_empty());
    }
}
