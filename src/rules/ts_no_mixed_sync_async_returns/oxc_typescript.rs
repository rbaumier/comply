//! ts-no-mixed-sync-async-returns OXC backend.
//!
//! Flags a non-async function whose body returns both a synchronous value and a
//! Promise. An explicit `T | Promise<T>` return-type annotation is an
//! intentional dual-mode contract (a handler returns sync when it can, async
//! only when it must) and is not flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::Function(func) => {
                check_function_body_fn(func, ctx, diagnostics);
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                check_arrow_body(arrow, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

/// Check a `Function` node (covers function_declaration, function_expression,
/// method_definition) for mixed sync/async returns.
fn check_function_body_fn(
    func: &Function,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if func.r#async {
        return;
    }
    let Some(body) = &func.body else { return };

    let mut has_sync = false;
    let mut has_async = false;
    collect_returns_from_stmts(&body.statements, ctx.source, &mut has_sync, &mut has_async);

    if has_sync && has_async {
        let (line, column) = byte_offset_to_line_col(ctx.source, func.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Function returns both a sync value and a Promise; mark it `async` so callers always `await`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn check_arrow_body(
    arrow: &ArrowFunctionExpression,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if arrow.r#async {
        return;
    }
    // Only check arrow functions with statement bodies (not expression bodies)
    if arrow.expression {
        return;
    }

    let mut has_sync = false;
    let mut has_async = false;
    collect_returns_from_stmts(&arrow.body.statements, ctx.source, &mut has_sync, &mut has_async);

    if has_sync && has_async {
        let (line, column) = byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Function returns both a sync value and a Promise; mark it `async` so callers always `await`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn collect_returns_from_stmts(
    stmts: &[Statement],
    source: &str,
    has_sync: &mut bool,
    has_async: &mut bool,
) {
    for stmt in stmts {
        collect_returns_stmt(stmt, source, has_sync, has_async);
    }
}

fn collect_returns_stmt(
    stmt: &Statement,
    source: &str,
    has_sync: &mut bool,
    has_async: &mut bool,
) {
    match stmt {
        // Don't descend into nested functions
        Statement::FunctionDeclaration(_) => {}
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                match classify_return_expr(arg, source) {
                    ReturnKind::Sync => *has_sync = true,
                    ReturnKind::Async => *has_async = true,
                    ReturnKind::Unknown => {}
                }
            }
        }
        Statement::BlockStatement(block) => {
            collect_returns_from_stmts(&block.body, source, has_sync, has_async);
        }
        Statement::IfStatement(if_stmt) => {
            collect_returns_stmt(&if_stmt.consequent, source, has_sync, has_async);
            if let Some(alt) = &if_stmt.alternate {
                collect_returns_stmt(alt, source, has_sync, has_async);
            }
        }
        Statement::ForStatement(f) => {
            collect_returns_stmt(&f.body, source, has_sync, has_async);
        }
        Statement::ForInStatement(f) => {
            collect_returns_stmt(&f.body, source, has_sync, has_async);
        }
        Statement::ForOfStatement(f) => {
            collect_returns_stmt(&f.body, source, has_sync, has_async);
        }
        Statement::WhileStatement(w) => {
            collect_returns_stmt(&w.body, source, has_sync, has_async);
        }
        Statement::DoWhileStatement(d) => {
            collect_returns_stmt(&d.body, source, has_sync, has_async);
        }
        Statement::SwitchStatement(s) => {
            for case in &s.cases {
                collect_returns_from_stmts(&case.consequent, source, has_sync, has_async);
            }
        }
        Statement::TryStatement(t) => {
            collect_returns_from_stmts(&t.block.body, source, has_sync, has_async);
            if let Some(handler) = &t.handler {
                collect_returns_from_stmts(&handler.body.body, source, has_sync, has_async);
            }
            if let Some(finalizer) = &t.finalizer {
                collect_returns_from_stmts(&finalizer.body, source, has_sync, has_async);
            }
        }
        Statement::LabeledStatement(l) => {
            collect_returns_stmt(&l.body, source, has_sync, has_async);
        }
        Statement::ExpressionStatement(_) => {}
        _ => {}
    }
}

enum ReturnKind {
    Sync,
    Async,
    Unknown,
}

fn classify_return_expr(expr: &Expression, source: &str) -> ReturnKind {
    match expr {
        Expression::AwaitExpression(_) => ReturnKind::Async,
        Expression::NewExpression(new) => {
            let ctor_text = &source[new.callee.span().start as usize..new.callee.span().end as usize];
            if ctor_text == "Promise" {
                ReturnKind::Async
            } else {
                ReturnKind::Sync
            }
        }
        Expression::CallExpression(call) => {
            let callee_text = &source[call.callee.span().start as usize..call.callee.span().end as usize];
            if callee_text.starts_with("Promise.") {
                ReturnKind::Async
            } else {
                ReturnKind::Unknown
            }
        }
        Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::TemplateLiteral(_)
        | Expression::ArrayExpression(_)
        | Expression::ObjectExpression(_) => ReturnKind::Sync,
        Expression::Identifier(_) => ReturnKind::Sync,
        _ => ReturnKind::Unknown,
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
    fn allows_mixed_in_type_alias_function() {
        let src = "type F = (a: number) => Promise<void> | void;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mixed_in_generic_type_alias_function() {
        let src = "type G = <T>(x: T) => T | Promise<T>;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mixed_in_interface_property_and_method_signature() {
        let src = "interface I { transform?: (r: R) => Promise<E[]> | E[]; m(): Promise<x> | x; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_concrete_function_body_mixing_returns() {
        let src = "function f(c: boolean): Promise<number> | number { if (c) return 1; return Promise.resolve(2); }";
        assert!(!run(src).is_empty());
    }

    #[test]
    fn allows_local_let_variable_annotation_mixing_promise_and_undefined() {
        // The union is the type of a local `let` binding inside an async
        // function, not the function's return type (issue #3902).
        let src = "async function run(): Promise<void> { let releasePrerequisite: Promise<unknown> | undefined; releasePrerequisite = Promise.resolve(1); await releasePrerequisite; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_class_field_annotation_mixing_promise_and_non_promise() {
        let src = "class C { x: number | Promise<number>; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_top_level_const_annotation_mixing_promise_and_non_promise() {
        let src = "const y: string | Promise<string> = foo();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_explicit_dual_mode_return_type() {
        // An explicit `T | Promise<T>` return annotation is an intentional
        // dual-mode contract — the body returns a single value, so the body-scan
        // path does not fire (issue #3779).
        let src = "function f(cond: boolean): string | Promise<string> { return cond ? \"x\" : Promise.resolve(\"y\"); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_explicit_dual_mode_arrow_html() {
        // hono's `html` helper: explicit `Resp | Promise<Resp>` dual-mode
        // contract on an arrow (issue #3779).
        let src = "type Resp = unknown; const html = (input: string | Promise<string>): Resp | Promise<Resp> => { return typeof input === 'object' ? input.then((s) => s) : input; };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_explicit_dual_mode_method_no_mirroring_param() {
        // hono's `#handleError`/`#dispatch`: explicit union return, no mirroring
        // `T | Promise<T>` parameter (issue #3779).
        let src = "class C { handle(err: Error, c: Ctx): Response | Promise<Response> { return c.json(err); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_explicit_dual_mode_passthrough_arrow() {
        // hono vercel adapter: pass-through arrow with explicit union return
        // (issue #3779).
        let src = "const h = (req: Request): Response | Promise<Response> => app.fetch(req);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_callback_parameter_type_with_mixed_return() {
        // The union `boolean | Promise<boolean>` is the return type of the
        // callback parameter TYPE `() => ...`, not of the enclosing async
        // function. Must not flag (issue #1149).
        let src = "export async function checkWithTimeout(predicate: () => boolean | Promise<boolean>, delay = 1000): Promise<boolean> { return await predicate(); }";
        assert!(run(src).is_empty());
    }
}
