//! ts-no-loop-func OXC backend — flag functions and arrow functions inside a
//! loop body that capture a binding the enclosing loop introduces (a symbol
//! declared by the loop header or in the loop body). Such a closure can read a
//! stale value when invoked on a later iteration. A closure that references only
//! its own params/locals or bindings declared outside the loop captures nothing
//! loop-scoped and is left alone. The `let`/`const` initializer of a C-style
//! `for (let i …)` is also exempt: ES2015 §14.7.4.2 re-binds it fresh each
//! iteration, so a closure capturing it reads its own value, not a shared one.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ForStatementInit, VariableDeclarationKind};
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

/// Byte span of the `let`/`const` initializer of a C-style `for (let i …)` /
/// `for (const x …)` loop — the per-iteration binding(s). `None` for a `var`
/// init, for an init that is not a variable declaration, and for any non-`for`
/// loop. Per ES2015 §14.7.4.2 a `let`/`const` `for` init is re-bound fresh each
/// iteration, so a closure capturing it reads its own value; a `var` init keeps
/// one function-scoped binding mutated across iterations and stays a hazard.
fn per_iteration_for_init_span(loop_kind: AstKind) -> Option<oxc_span::Span> {
    let AstKind::ForStatement(stmt) = loop_kind else {
        return None;
    };
    let Some(ForStatementInit::VariableDeclaration(decl)) = &stmt.init else {
        return None;
    };
    matches!(
        decl.kind,
        VariableDeclarationKind::Let | VariableDeclarationKind::Const
    )
    .then_some(decl.span)
}

/// True when the closure spanning `func_span` references at least one binding the
/// enclosing loop introduces: a symbol whose declaration sits inside the loop
/// span (its header or body) but outside the closure itself, and that the closure
/// references. That capture is the stale-shared-binding hazard this rule targets.
/// A symbol declared inside the closure is the closure's own param/local; a symbol
/// declared above the loop is stable across iterations — neither contributes. The
/// `let`/`const` initializer of a C-style `for` is also excluded: it is re-bound
/// per iteration, so capturing it is sound.
fn captures_loop_binding<'a>(
    func_span: oxc_span::Span,
    loop_kind: AstKind<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let loop_span = loop_kind.span();
    let per_iteration_init = per_iteration_for_init_span(loop_kind);
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    for sym_id in scoping.symbol_ids() {
        let decl_span = scoping.symbol_span(sym_id);
        let is_loop_binding =
            span_contains(loop_span, decl_span) && !span_contains(func_span, decl_span);
        if !is_loop_binding {
            continue;
        }
        // The `let`/`const` loop variable of a C-style `for` is re-bound each
        // iteration, so capturing it reads that iteration's own value — not the
        // shared-binding hazard. `var` inits and bindings declared in the loop
        // body remain hazards.
        if per_iteration_init.is_some_and(|init_span| span_contains(init_span, decl_span)) {
            continue;
        }
        for reference in scoping.get_resolved_references(sym_id) {
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
    fn flags_closure_capturing_let_declared_in_loop_body() {
        // `n` is declared inside the loop body and captured by the closure — a
        // fresh value per iteration that the deferred closure reads stale.
        let src = r#"
            for (let i = 0; i < 10; i++) {
                let n = next();
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
    fn flags_closure_capturing_loop_var_in_while() {
        // A `let` declared and mutated in a `while` body, captured by a stored
        // closure — the classic deferred-read hazard.
        let src = r#"
            let i = 0;
            while (i < 10) {
                let n = i;
                fns.push(() => n);
                i++;
            }
        "#;
        let d = run(src, "src/app.ts");
        assert_eq!(d.len(), 1);
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
}
