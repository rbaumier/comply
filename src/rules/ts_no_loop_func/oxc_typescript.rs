//! ts-no-loop-func OXC backend — flag functions and arrow functions inside a
//! loop body that capture a `var` the enclosing loop shares across iterations:
//! one hoisted, function-scoped binding the loop mutates, so a closure invoked on
//! a later iteration reads a stale value. A closure that references only its own
//! params/locals, a binding declared above the loop, or a `let`/`const` declared
//! within the loop is left alone. A block-scoped `let`/`const` — whether in the
//! C-style `for` init, a `for-in` header, or the loop body — is re-bound fresh each
//! iteration (ES2015), so the closure captures its own value, not a value shared
//! with later iterations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::VariableDeclarationKind;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_loop_kind(kind: AstKind) -> bool {
    matches!(
        kind,
        AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_)
    )
}

fn is_function_boundary(kind: AstKind) -> bool {
    matches!(
        kind,
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
    )
}

/// Nearest enclosing loop of `node`, or `None` if a function boundary is reached
/// first — an intervening function decouples the closure from any outer loop
/// binding, so the outer loop is not the closure's own iteration context.
fn enclosing_loop<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<AstKind<'a>> {
    let mut first = true;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        // Skip the node itself.
        if first {
            first = false;
            continue;
        }
        let kind = ancestor.kind();
        // Stop at function boundaries — nested functions don't count.
        if is_function_boundary(kind) {
            return None;
        }
        if is_loop_kind(kind) {
            return Some(kind);
        }
    }
    None
}

/// True when `inner` is fully contained within `outer` (byte-span nesting).
fn span_contains(outer: oxc_span::Span, inner: oxc_span::Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// True when `sym_id` is declared by a block-scoped `let`/`const`
/// `VariableDeclaration`. A `let`/`const` binding is block-scoped: every execution
/// of its enclosing block — including every loop iteration — produces a fresh
/// binding, so a closure created in that iteration captures that iteration's own
/// value, never one shared with later iterations. Resolves the symbol's declaration
/// node and walks up to its nearest enclosing `VariableDeclaration` to read the
/// kind, which also covers destructuring (`const { a } = …`), where the binding sits
/// inside an `ObjectPattern`/`ArrayPattern` under the declarator. Returns `false` for
/// `var` (one hoisted, function-scoped binding shared across iterations) and for
/// non-variable declarations.
fn is_block_scoped_declared_symbol(
    sym_id: oxc_semantic::SymbolId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let decl_node_id = semantic.scoping().symbol_declaration(sym_id);
    std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
        .find_map(|kind| match kind {
            AstKind::VariableDeclaration(decl) => Some(matches!(
                decl.kind,
                VariableDeclarationKind::Let | VariableDeclarationKind::Const
            )),
            _ => None,
        })
        .unwrap_or(false)
}

/// True when the closure spanning `func_span` captures at least one binding the
/// enclosing loop shares across iterations: a `var` whose declaration sits inside
/// the loop span (its header or body) but outside the closure itself, and that the
/// closure references. A `var` is one hoisted, function-scoped binding the loop
/// mutates, so a deferred closure reads its final value — the hazard this rule
/// targets. A `let`/`const` declared anywhere within the loop (the C-style `for`
/// init, a `for-in` header, or the loop body) is block-scoped and re-bound fresh
/// each iteration, so the closure captures that iteration's own value and is sound.
/// A symbol declared inside the closure is its own param/local; one declared above
/// the loop is stable across iterations — neither contributes.
fn captures_loop_binding<'a>(
    func_span: oxc_span::Span,
    loop_kind: AstKind<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let loop_span = loop_kind.span();
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    for sym_id in scoping.symbol_ids() {
        let decl_span = scoping.symbol_span(sym_id);
        let is_loop_binding =
            span_contains(loop_span, decl_span) && !span_contains(func_span, decl_span);
        if !is_loop_binding {
            continue;
        }
        // A `let`/`const` declared within the loop — the C-style `for` init, a
        // `for-in` header, or the loop body — is block-scoped and re-bound fresh on
        // every iteration (ES2015 per-iteration / block environments), so a closure
        // created that iteration captures that iteration's own binding, not a value
        // shared with later ones. Only a `var` (one hoisted, function-scoped binding
        // mutated across iterations) stays a hazard.
        if is_block_scoped_declared_symbol(sym_id, semantic) {
            continue;
        }
        for reference in scoping.get_resolved_references(sym_id) {
            // A type-only reference (`typeof binding` in a type annotation, `: T`,
            // `as T`) is erased at compile time: the emitted JS holds no reference
            // to the binding, so the closure captures nothing at runtime and cannot
            // read a stale value. Only a value-position reference forms a real
            // capture. `get_resolved_references` yields both, so skip type-only ones.
            if !reference.flags().is_value() {
                continue;
            }
            let ref_span = nodes.kind(reference.node_id()).span();
            if span_contains(func_span, ref_span) {
                return true;
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(loop_kind) = enclosing_loop(node, semantic) else {
            return;
        };

        let span = match node.kind() {
            AstKind::Function(f) => f.span,
            AstKind::ArrowFunctionExpression(a) => a.span,
            _ => return,
        };

        // Only a closure that closes over a loop-introduced binding can read a
        // stale value on a later iteration. One that touches only its own
        // params/locals or bindings declared above the loop is sound.
        if !captures_loop_binding(span, loop_kind, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Function declared inside a loop captures the loop variable by \
                      reference and may read a stale value. Move it outside."
                .into(),
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

    fn run(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn allows_non_capturing_callbacks_in_c_style_for() {
        // Issue #6398: fire-and-forget I/O callbacks inside `for (let i = …)`
        // that reference none of the loop bindings — no stale-capture hazard.
        let src = r#"
            async function poll() {
                for (let i = 0; i < 100; i++) {
                    if (await fetch(url).then(r => r.ok).catch(() => false)) break;
                    await new Promise(resolve => setTimeout(resolve, 500));
                }
            }
        "#;
        let d = run(src, "src/app.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_arrow_capturing_per_iteration_let_loop_var() {
        // Issue #6473: `let` in a C-style `for` creates a fresh per-iteration
        // binding (ES2015 §14.7.4.2), so each closure captures its own `i` —
        // `fns.map(f => f())` yields [0..9], not [10,…]. No stale-closure hazard.
        let src = r#"
            for (let i = 0; i < 10; i++) {
                fns.push(() => i);
            }
        "#;
        let d = run(src, "src/app.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_closures_capturing_let_loop_var_unjs_hookable_repro() {
        // Issue #6473 real-world repro (unjs/hookable src/utils.ts). Both arrows
        // capture only `i` (the per-iteration `let` loop var) and bindings
        // declared above the loop (`hooks`, `args`, `task`, `callHooks`).
        let src = r#"
            function callHooks(hooks, args, startIndex, task) {
                for (let i = startIndex; i < hooks.length; i += 1) {
                    const result = task ? task.run(() => hooks[i](...args)) : hooks[i](...args);
                    if (result && typeof result.then === "function") {
                        return Promise.resolve(result).then(() => callHooks(hooks, args, i + 1, task));
                    }
                }
            }
        "#;
        let d = run(src, "src/utils.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_arrow_capturing_per_iteration_const_loop_var() {
        // A C-style `for` with a `const` init also re-binds per iteration.
        let src = r#"
            for (const x = seed(); cond(x); ) {
                fns.push(() => x);
            }
        "#;
        let d = run(src, "src/app.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn flags_arrow_capturing_var_loop_var() {
        // Negative-space guard: `var` is a single function-scoped binding mutated
        // across iterations, so every closure shares it and reads the final value
        // — the classic stale-closure bug, which must STILL be flagged.
        let src = r#"
            for (var i = 0; i < n; i++) {
                fns.push(() => i);
            }
        "#;
        let d = run(src, "src/app.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_closure_capturing_let_declared_in_loop_body() {
        // Issue #6907: `n` is a block-scoped `let` declared in the loop body, so
        // each iteration binds a fresh `n` and each closure captures its own
        // value — `fns.map(f => f())` yields each iteration's `next()`, not a
        // shared one. No stale-capture hazard.
        let src = r#"
            for (let i = 0; i < 10; i++) {
                let n = next();
                fns.push(() => n);
            }
        "#;
        let d = run(src, "src/app.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn flags_closure_capturing_var_declared_in_loop_body() {
        // Negative-space guard: `var n` in the loop body is one hoisted,
        // function-scoped binding shared and mutated across iterations, so every
        // stored closure reads the final value — the classic stale-closure bug,
        // which must STILL be flagged.
        let src = r#"
            for (let i = 0; i < 10; i++) {
                var n = next();
                fns.push(() => n);
            }
        "#;
        let d = run(src, "src/app.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_closure_capturing_only_outer_binding() {
        // `outer` is declared above the loop — stable across iterations, not a
        // loop binding — so capturing it is sound.
        let src = r#"
            const outer = 1;
            for (let i = 0; i < 10; i++) {
                fns.push(() => outer);
            }
        "#;
        let d = run(src, "src/app.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_closure_capturing_const_in_while_body() {
        // Issue #6907 (vuejs/core compiler-core/src/transforms/vIf.ts repro):
        // `key` is a block-scoped `const` declared in the `while` body, so each
        // iteration binds a fresh `key` and the captured arrow reads its own
        // iteration's value. `userKey` is the arrow's own destructured param.
        let src = r#"
            while (i-- >= -1) {
                const key = branch.userKey;
                if (key) {
                    sibling.branches.forEach(({ userKey }) => {
                        if (isSameKey(userKey, key)) {
                            context.onError();
                        }
                    });
                }
            }
        "#;
        let d = run(src, "src/vIf.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_closure_capturing_const_in_c_style_for_body() {
        // Issue #6907 (vuejs/core runtime-dom/src/directives/vModel.ts repro):
        // `optionValue` is a block-scoped `const` in the `for` body, fresh each
        // iteration, captured by the `some` callback. `v` is the callback's param.
        let src = r#"
            for (let i = 0, l = el.options.length; i < l; i++) {
                const optionValue = getValue(el.options[i]);
                const optionType = typeof optionValue;
                if (optionType === 'string' || optionType === 'number') {
                    option.selected = value.some(v => String(v) === String(optionValue));
                }
            }
        "#;
        let d = run(src, "src/vModel.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_closure_capturing_for_in_header_binding() {
        // Issue #6907 (vuejs/core runtime-dom/src/apiCustomElement.ts repro):
        // `key` is the `for-in` header `const`, a fresh per-iteration binding, so
        // the getter arrow captures its own iteration's `key`. `exposed` is an
        // outer binding, stable across iterations.
        let src = r#"
            for (const key in exposed) {
                Object.defineProperty(this, key, {
                    get: () => unref(exposed[key]),
                });
            }
        "#;
        let d = run(src, "src/apiCustomElement.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_closure_capturing_destructured_const_in_loop_body() {
        // Issue #6907: a destructured block-scoped `const` (`const { id } = …`) in
        // the loop body is re-bound fresh each iteration, so the stored closure
        // captures its own iteration's `id`. The binding sits inside an
        // `ObjectPattern`, so the exemption must resolve through the pattern up to
        // the enclosing `VariableDeclaration` to read its kind.
        let src = r#"
            for (let i = 0; i < items.length; i++) {
                const { id } = items[i];
                fns.push(() => id);
            }
        "#;
        let d = run(src, "src/app.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_polling_delay_promise_in_while() {
        // The arrow captures only its own `resolve` param and the global
        // `setTimeout` — nothing loop-scoped.
        let src = r#"
            while (!done) {
                await new Promise(resolve => setTimeout(resolve, 10));
            }
        "#;
        let d = run(src, "src/app.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_closure_referencing_loop_var_only_in_type_annotation() {
        // Issue #7124: the arrow's only reference to the function-scoped `var
        // promise` is `typeof promise` in a type annotation. Type-position
        // references are erased at compile time — the emitted JS never reads
        // `promise` inside the closure, so there is no runtime capture and no
        // stale-binding hazard. A `var` binding here isolates the type-only skip
        // (a `let` would already be exempted as block-scoped).
        let src = r#"
            function g() {
                while (cond()) {
                    var promise = read();
                    cb(async () => {
                        const res: Awaited<typeof promise> = build();
                        return res;
                    });
                }
            }
        "#;
        let d = run(src, "src/app.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_trpc_sse_let_promise_type_annotation_repro() {
        // Issue #7124 real-world repro (trpc/trpc stream/sse.ts): `promise` is a
        // block-scoped `let` referenced inside the `onTimeout` arrow only via
        // `typeof promise` in a type annotation. Sound on both counts — fresh
        // per-iteration binding and a type-only, erased reference.
        let src = r#"
            while (true) {
                let promise = stream.read();
                promise = withTimeout({
                    promise,
                    onTimeout: async () => {
                        const res: Awaited<typeof promise> = { done: false };
                        await stream.recreate();
                        return res;
                    },
                });
                const result = await promise;
            }
        "#;
        let d = run(src, "src/sse.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn flags_closure_capturing_var_in_value_position() {
        // Negative-space guard: the arrow reads the function-scoped `var promise`
        // as a value (`await promise`) — a real runtime capture of a binding
        // mutated across iterations, which must STILL be flagged.
        let src = r#"
            function g() {
                while (cond()) {
                    var promise = read();
                    cb(async () => {
                        const res = await promise;
                        return res;
                    });
                }
            }
        "#;
        let d = run(src, "src/app.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_closure_capturing_var_in_both_value_and_type_position() {
        // Negative-space guard: the arrow references the `var promise` in a type
        // position (`typeof promise`) AND a value position (`await promise`). The
        // value reference is a real capture, so the closure must STILL be flagged
        // — the type-only skip must not suppress a genuine value capture.
        let src = r#"
            function g() {
                while (cond()) {
                    var promise = read();
                    cb(async () => {
                        const res: Awaited<typeof promise> = await promise;
                        return res;
                    });
                }
            }
        "#;
        let d = run(src, "src/app.ts");
        assert_eq!(d.len(), 1);
    }
}
