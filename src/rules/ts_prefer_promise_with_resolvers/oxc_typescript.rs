//! ts-prefer-promise-with-resolvers oxc backend.
//!
//! Flags `new Promise(executor)` only when the executor leaks its `resolve`/
//! `reject` parameter out of its own scope — assigning it to an outer-scoped
//! binding, storing it on an object (`this.x = resolve`), or returning it.
//! Those are exactly the cases `Promise.withResolvers()` was designed for: it
//! hands back `{ promise, resolve, reject }` so the settle handles can be called
//! from outside the executor.
//!
//! A self-contained executor — one that only calls `resolve`/`reject` or passes
//! them to APIs it drives itself (`setTimeout(resolve, ms)`,
//! `child.on("close", () => resolve(x))`) — is the idiomatic use of the
//! constructor and is left alone; `withResolvers` would only make it longer.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, AssignmentTarget, BindingPattern, Expression, FormalParameter};
use oxc_semantic::{NodeId, Semantic, SymbolId};
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

pub struct Check;

/// The `SymbolId` of a formal parameter, if it's a plain identifier binding
/// (not a destructuring or rest pattern — those can't be the settle handle).
fn param_symbol(param: &FormalParameter) -> Option<SymbolId> {
    match &param.pattern {
        BindingPattern::BindingIdentifier(id) => id.symbol_id.get(),
        _ => None,
    }
}

fn span_contains(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// True when `symbol`'s declaration lies outside `executor_span` — i.e. the
/// binding it names was declared in an enclosing scope, so assigning the
/// settle handle to it makes the handle escape the executor.
fn declared_outside(semantic: &Semantic, symbol: SymbolId, executor_span: Span) -> bool {
    let decl_id = semantic.scoping().symbol_declaration(symbol);
    let decl_span = semantic.nodes().kind(decl_id).span();
    !span_contains(executor_span, decl_span)
}

/// True when assigning to `target` makes the assigned value escape the executor:
/// either the target is a member expression (`this.x`, `obj.x`) — a store onto
/// an object that outlives the executor — or it's an identifier bound outside
/// the executor's span.
fn assignment_target_escapes(
    target: &AssignmentTarget,
    semantic: &Semantic,
    executor_span: Span,
) -> bool {
    match target {
        AssignmentTarget::AssignmentTargetIdentifier(id) => id
            .reference_id
            .get()
            .and_then(|ref_id| semantic.scoping().get_reference(ref_id).symbol_id())
            // An unresolvable LHS (e.g. an undeclared global) can't be confirmed
            // to be outer-scoped, so conservatively treat it as non-escaping.
            .is_some_and(|symbol| declared_outside(semantic, symbol, executor_span)),
        // Any member-expression target stores the handle on an object.
        _ if target.as_member_expression().is_some() => true,
        _ => false,
    }
}

/// Span of the function that most tightly encloses `node_id`, or `None` at the
/// top level.
fn enclosing_function_span(node_id: NodeId, semantic: &Semantic) -> Option<Span> {
    semantic
        .nodes()
        .ancestors(node_id)
        .find_map(|ancestor| match ancestor.kind() {
            AstKind::Function(f) => Some(f.span),
            AstKind::ArrowFunctionExpression(a) => Some(a.span),
            _ => None,
        })
}

/// True when reference node `ref_id` is an escape of the settle handle out of
/// the executor: it is the right-hand side of an escaping assignment, or it is
/// returned directly from the executor.
///
/// References that are call targets (`resolve(x)`) or arguments to other calls
/// (`setTimeout(resolve, ms)`) are *not* escapes — the handle stays inside the
/// executor's control flow.
fn reference_escapes(ref_id: NodeId, semantic: &Semantic, executor_span: Span) -> bool {
    let nodes = semantic.nodes();
    let ref_span = nodes.kind(ref_id).span();
    match nodes.parent_node(ref_id).kind() {
        AstKind::AssignmentExpression(assign) => {
            // Only the RHS escapes; `resolve = x` (handle as LHS) does not.
            span_contains(assign.right.span(), ref_span)
                && assignment_target_escapes(&assign.left, semantic, executor_span)
        }
        // `return resolve;` — only when it returns out of the executor itself,
        // not out of a nested function whose result is discarded.
        AstKind::ReturnStatement(_) => {
            enclosing_function_span(ref_id, semantic) == Some(executor_span)
        }
        _ => false,
    }
}

/// True when any reference to `symbol` inside `executor_span` leaks the handle
/// out of the executor.
fn symbol_escapes(semantic: &Semantic, symbol: SymbolId, executor_span: Span) -> bool {
    let nodes = semantic.nodes();
    semantic
        .scoping()
        .get_resolved_references(symbol)
        .any(|reference| {
            let ref_id = reference.node_id();
            span_contains(executor_span, nodes.kind(ref_id).span())
                && reference_escapes(ref_id, semantic, executor_span)
        })
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };
        // Only the global `Promise` constructor — not `Foo.Promise`, and not a
        // `Promise.resolve()` call (which is a CallExpression, never seen here).
        let Expression::Identifier(ctor) = &new_expr.callee else {
            return;
        };
        if ctor.name.as_str() != "Promise" {
            return;
        }

        let Some(arg) = new_expr.arguments.first() else {
            return;
        };
        let (params, executor_span) = match arg {
            Argument::ArrowFunctionExpression(a) => (&a.params, a.span),
            Argument::FunctionExpression(f) => (&f.params, f.span),
            // A non-inline executor (e.g. a passed-in function reference) gives
            // us no body to analyse — leave it alone.
            _ => return,
        };

        let resolve = params.items.first().and_then(param_symbol);
        let reject = params.items.get(1).and_then(param_symbol);

        let escapes = [resolve, reject]
            .into_iter()
            .flatten()
            .any(|symbol| symbol_escapes(semantic, symbol, executor_span));
        if !escapes {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "This executor leaks `resolve`/`reject` out of its scope — prefer \
                      `Promise.withResolvers()` to get `{ promise, resolve, reject }` \
                      without an executor closure."
                .to_string(),
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

    // --- Escape cases: withResolvers genuinely helps -> flag ---

    #[test]
    fn flags_assignment_to_outer_let() {
        // `r = resolve` stores the handle in an outer-scoped binding so it can be
        // called later from outside the executor.
        let src = "let r; const p = new Promise((resolve) => { r = resolve; }); r();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_member_store() {
        let src = "new Promise((resolve) => { this.resolve = resolve; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_returned_handle() {
        let src = "let captured; new Promise((resolve) => { captured = 1; return resolve; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_reject_escape() {
        let src = "let rj; new Promise((resolve, reject) => { rj = reject; });";
        assert_eq!(run(src).len(), 1);
    }

    // --- Self-contained executors: withResolvers offers nothing -> no flag ---

    #[test]
    fn allows_settimeout_sleep() {
        // The canonical sleep idiom from cline auth/cline.ts.
        let src = "async function sleep(ms) { await new Promise((resolve) => setTimeout(resolve, ms)); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_callback_wrapping() {
        let src = "function f() { return new Promise((resolveMatches) => { const child = spawn(\"git\"); child.on(\"close\", () => resolveMatches(parse(stdout))); }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_direct_resolve_call() {
        let src = "const p = new Promise((resolve, reject) => resolve(1));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_resolve_reject_branches() {
        let src = "const p = new Promise((resolve, reject) => { if (ok) resolve(1); else reject(e); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_return_from_nested_function() {
        // `return resolve` here returns out of the `.map` callback, not the
        // executor; the value is discarded, so the handle never escapes.
        let src = "new Promise((resolve) => { [1].map(() => { return resolve; }); resolve(1); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_inner_let_capture() {
        // The assignment target is declared *inside* the executor, so the handle
        // never leaves it.
        let src = "new Promise((resolve) => { let local; local = resolve; local(1); });";
        assert!(run(src).is_empty());
    }

    // --- Not the global Promise constructor / not analysable -> no flag ---

    #[test]
    fn ignores_promise_resolve_static() {
        let src = "const p = Promise.resolve(42);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_inline_executor() {
        let src = "const p = new Promise(executor);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_member_promise() {
        let src = "let r; const p = new Foo.Promise((resolve) => { r = resolve; });";
        assert!(run(src).is_empty());
    }
}
