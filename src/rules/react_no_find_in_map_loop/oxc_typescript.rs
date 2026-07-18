//! react-no-find-in-map-loop OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::oxc_helpers::{byte_offset_to_line_col, root_identifier_of_expr, span_contains};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// The rule's rationale is render-path (O(n²)) cost, so it only applies to
/// React code: `.tsx`/`.jsx` files (JSX implies React) or a `.ts`/`.js` module
/// that imports React. Plain backend/server TypeScript is out of scope.
fn in_react_context(ctx: &CheckCtx) -> bool {
    matches!(ctx.lang, Language::Tsx) || crate::oxc_helpers::imports_react(ctx.source)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["find", "filter"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !in_react_context(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Must be a `.find(...)` or `.filter(...)` member call.
        let Expression::StaticMemberExpression(mem) = &call.callee else {
            return;
        };
        let method = mem.property.name.as_str();
        if method != "find" && method != "filter" {
            return;
        }

        // Check if it's inside a loop or .map() callback.
        if !flagged_inside_loop_or_map(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.find`/`.filter` inside a `.map` or loop — O(n\u{b2}). \
                      Build a `Map` once and look up inside the loop."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Walk up from the find/filter `node` to decide whether it is a per-iteration,
/// correlated O(n²) scan.
///
/// The scan runs per iteration only when its nearest enclosing function is directly
/// the callback of the loop/`.map()`. An intervening nested function (an event
/// handler such as `onRemove`, a `useCallback`, any deferred closure stored in a
/// prop/variable) means the call is deferred — it runs once on user interaction,
/// not once per iteration — so there is no O(n²) render cost; the walk bails the
/// moment it crosses a function boundary that is not the loop/map's own callback.
///
/// Being per-iteration is necessary but not sufficient. The scan is O(n²) only when
/// it is *correlated* with the iteration binding and re-scans a *loop-invariant*
/// collection: the predicate references the iteration binding while the receiver's
/// root is declared outside the loop/map body (see [`is_correlated_on2_scan`]). A
/// receiver rooted on the iteration binding or a per-iteration local scans an
/// element-local collection (linear), and a predicate independent of the binding is
/// loop-invariant (same result every iteration); neither is flagged, and the walk
/// keeps climbing in case an outer loop/map makes the scan correlated. A `while`/
/// `do-while` declares no iteration binding, so it is never on its own the flagged
/// level.
fn flagged_inside_loop_or_map(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let AstKind::CallExpression(find_call) = node.kind() else {
        return false;
    };
    let receiver_root = find_receiver_root_symbol(find_call, semantic);
    let predicate_span = find_predicate_span(find_call);

    let mut current = node.id();
    loop {
        let parent_id = semantic.nodes().parent_id(current);
        if parent_id == current {
            return false;
        }
        let child = current;
        current = parent_id;
        let parent = semantic.nodes().get_node(current);
        match parent.kind() {
            AstKind::ForOfStatement(stmt) => {
                if is_correlated_on2_scan(
                    stmt.left.span(),
                    stmt.span,
                    receiver_root,
                    predicate_span,
                    semantic,
                ) {
                    return true;
                }
            }
            AstKind::ForInStatement(stmt) => {
                if is_correlated_on2_scan(
                    stmt.left.span(),
                    stmt.span,
                    receiver_root,
                    predicate_span,
                    semantic,
                ) {
                    return true;
                }
            }
            AstKind::ForStatement(stmt) => {
                if let Some(binding_span) = for_init_binding_span(stmt)
                    && is_correlated_on2_scan(
                        binding_span,
                        stmt.span,
                        receiver_root,
                        predicate_span,
                        semantic,
                    )
                {
                    return true;
                }
            }
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                // Crossing a function boundary that is itself a loop/`.map()`
                // callback is the expected per-iteration case — keep walking up.
                // Any other enclosing function (handler, deferred closure) means
                // the find/filter is deferred, not per-iteration: stop.
                if !is_loop_or_map_callback(current, semantic) {
                    return false;
                }
            }
            AstKind::CallExpression(call) => {
                if is_map_call(call) {
                    // Distinguish how the walk reached this `.map`. If we ascended
                    // from its callee (the `X.map` member expression), the
                    // find/filter is in the receiver chain — `arr.filter(...).map(...)`
                    // — i.e. downstream chaining (two sequential O(n) passes), not
                    // nesting. If we ascended from within the arguments (the
                    // callback subtree), it's genuine per-iteration nesting.
                    let child_span = semantic.nodes().get_node(child).kind().span();
                    if child_span == call.callee.span() {
                        return false;
                    }
                    // Inside the callback: flag only a correlated scan over a
                    // loop-invariant collection (see [`is_correlated_on2_scan`]).
                    // Otherwise keep walking up — an outer loop/map may correlate.
                    if let Some((binding_span, body_span)) = map_correlation_spans(call)
                        && is_correlated_on2_scan(
                            binding_span,
                            body_span,
                            receiver_root,
                            predicate_span,
                            semantic,
                        )
                    {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
}

/// True when the function node `func_id` is the callback argument of a `.map()`
/// call — i.e. its parent is a `.map()` CallExpression and the function sits in
/// that call's arguments. This is the one function boundary that runs per
/// iteration; every other enclosing function defers execution.
fn is_loop_or_map_callback(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let func_span = semantic.nodes().get_node(func_id).kind().span();
    let parent = semantic.nodes().parent_node(func_id);
    let AstKind::CallExpression(call) = parent.kind() else {
        return false;
    };
    if !is_map_call(call) {
        return false;
    }
    call.arguments
        .iter()
        .filter_map(oxc_ast::ast::Argument::as_expression)
        .any(|arg| arg.span() == func_span)
}

fn is_map_call(call: &oxc_ast::ast::CallExpression) -> bool {
    if let Expression::StaticMemberExpression(mem) = &call.callee {
        mem.property.name == "map"
    } else {
        false
    }
}

/// True when the find/filter is a correlated O(n²) scan relative to an iteration
/// binding: its predicate references the binding declared in `binding_span` **and**
/// its receiver is loop-invariant.
///
/// The receiver is loop-invariant when its root identifier is declared outside
/// `body_span` — a collection fixed across iterations, re-scanned in full every
/// time. A root declared inside `body_span` (the iteration binding itself, or a
/// per-iteration local) scans an element-local collection that differs each
/// iteration, so total work is Σ of the element sizes — linear, not O(n²). An
/// unresolvable receiver root (a global, an import, or a call result) is treated as
/// loop-invariant.
///
/// The binding is matched by resolved symbol, not by name: a binding qualifies when
/// its declaration sits inside `binding_span` and one of its resolved references
/// sits inside the predicate. This works for a plain identifier parameter
/// (`item => …`) and a destructured one (`({ id }) => …`) alike, and a predicate
/// that shadows the binding name with its own parameter is not mistaken for a
/// reference to the outer iteration binding.
fn is_correlated_on2_scan(
    binding_span: oxc_span::Span,
    body_span: oxc_span::Span,
    receiver_root: Option<oxc_semantic::SymbolId>,
    predicate_span: Option<oxc_span::Span>,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(predicate_span) = predicate_span else {
        return false;
    };
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let predicate_refs_binding = scoping.symbol_ids().any(|symbol_id| {
        span_contains(binding_span, scoping.symbol_span(symbol_id))
            && scoping.get_resolved_references(symbol_id).any(|reference| {
                span_contains(predicate_span, nodes.get_node(reference.node_id()).kind().span())
            })
    });
    if !predicate_refs_binding {
        return false;
    }
    receiver_root.is_none_or(|symbol_id| !span_contains(body_span, scoping.symbol_span(symbol_id)))
}

/// Resolved symbol of the root identifier of the find/filter receiver chain — the
/// base of the member expression the find/filter is called on (`entry` in
/// `entry.data?.values.filter(…)`, `stack` in `stack.filter(…)`). `None` when the
/// receiver is not a member/identifier chain (e.g. a call result `data.map(…).filter(…)`)
/// or its root does not resolve to a binding (a global or an import).
fn find_receiver_root_symbol(
    find_call: &oxc_ast::ast::CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::SymbolId> {
    let Expression::StaticMemberExpression(mem) = &find_call.callee else {
        return None;
    };
    let root = root_identifier_of_expr(&mem.object)?;
    root.reference_id
        .get()
        .and_then(|ref_id| semantic.scoping().get_reference(ref_id).symbol_id())
}

/// Span of the find/filter predicate — its first argument (the callback, or a
/// predicate function passed by reference). `None` for a call with no argument or a
/// spread first argument.
fn find_predicate_span(find_call: &oxc_ast::ast::CallExpression) -> Option<oxc_span::Span> {
    find_call
        .arguments
        .first()
        .and_then(oxc_ast::ast::Argument::as_expression)
        .map(|expr| expr.span())
}

/// Iteration binding span and body span of a `.map(cb)` call: the callback's first
/// parameter (the iteration element, whether a plain identifier or a destructuring
/// pattern) and the callback function's own span. Returns `None` for spreads,
/// non-function arguments, or a callback with no parameters.
fn map_correlation_spans(
    call: &oxc_ast::ast::CallExpression,
) -> Option<(oxc_span::Span, oxc_span::Span)> {
    let expr = call.arguments.first()?.as_expression()?;
    let (params, body_span) = match expr {
        Expression::ArrowFunctionExpression(arrow) => (&arrow.params, arrow.span),
        Expression::FunctionExpression(func) => (&func.params, func.span),
        _ => return None,
    };
    Some((params.items.first()?.span, body_span))
}

/// Span of the `let`/`const`/`var` declaration in a C-style `for (…;…;…)` header —
/// the counter binding. `None` when the init is absent or is a bare-expression init
/// that declares no binding.
fn for_init_binding_span(stmt: &oxc_ast::ast::ForStatement) -> Option<oxc_span::Span> {
    match &stmt.init {
        Some(oxc_ast::ast::ForStatementInit::VariableDeclaration(decl)) => Some(decl.span),
        _ => None,
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
    

    const NESTED: &str = r#"items.map(i => others.find(o => o.id === i.id));"#;

    #[test]
    fn flags_in_tsx() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, NESTED, "t.tsx").len(), 1);
    }

    // Regression for #280: a plain `.ts` module with no React import is not
    // render-path code — the rule must stay silent there.
    #[test]
    fn allows_plain_ts_without_react() {
        assert!(crate::rules::test_helpers::run_rule(&Check, NESTED, "service.ts").is_empty());
    }

    #[test]
    fn flags_ts_that_imports_react() {
        let src = format!("import {{ useMemo }} from \"react\";\n{NESTED}");
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, &src, "hook.ts").len(), 1);
    }

    // Regression for #911: a spread argument to .map() made `Argument::to_expression()` panic.
    #[test]
    fn does_not_panic_on_spread_arg_in_map() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "arr.map(...fns)", "t.tsx").is_empty());
    }

    // True positive: a `.filter()` directly in the map callback body (no
    // intervening function) is per-iteration — must still flag.
    #[test]
    fn flags_filter_directly_in_map_callback() {
        let src = "items.map(i => others.filter(o => o.id === i.id));";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").len(), 1);
    }

    // Regression for #3936: a `.filter()` inside a nested deferred event handler
    // (`onClick`) defined within the map callback runs once per user click, not
    // once per `.map()` iteration — no O(n²) render cost, so it must not flag.
    #[test]
    fn allows_filter_in_nested_event_handler() {
        let src = "arr.map((item) => <X onClick={() => other.filter((i) => i !== item)} />);";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // True positive: a `.filter()` directly in a `for`-loop body (no
    // intervening function) is per-iteration — must still flag.
    #[test]
    fn flags_filter_directly_in_for_loop() {
        let src = "for (const x of xs) { others.filter(o => o.id === x.id); }";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").len(), 1);
    }

    // Regression for #3936: a `.filter()` inside a deferred closure defined in a
    // `for`-loop body runs on later invocation, not per-iteration — must not flag.
    #[test]
    fn allows_filter_in_deferred_closure_in_for_loop() {
        let src = "for (const x of xs) { register(() => others.filter(o => o.id === x.id)); }";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // Regression for #3745: chained `arr.filter(...).map(...)` is two sequential
    // O(n) passes — the `.filter` is the `.map` receiver, not nested in its
    // callback. The walk reaches `.map` via its callee, so it must not flag.
    #[test]
    fn allows_chained_filter_then_map() {
        let src = "const r = data.filter((env) => ids.includes(env.id)).map((env) => env.name);";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // Regression for #3745: a longer chain `arr.filter(...).map(...).join(...)`
    // is still pure downstream chaining — must not flag.
    #[test]
    fn allows_longer_filter_map_chain() {
        let src = r#"const r = arr.filter((x) => x.ok).map((x) => x.id).join(",");"#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // Anti-over-broad guard for #3745: a genuinely nested `.find` inside the map
    // callback, with a downstream `.name` access on the find result, is still
    // O(n²). The fix only distinguishes callee vs arguments AT the `.map` call,
    // not at every member step, so this must still flag.
    #[test]
    fn flags_nested_find_with_downstream_access() {
        let src = "const r = items.map((i) => others.find((o) => o.id === i.id).name);";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").len(), 1);
    }

    // Regression for #3936: the mantine MultiSelect shape — a `.filter()` inside
    // an `onRemove` arrow nested in the map callback. Deferred, must not flag.
    #[test]
    fn allows_filter_in_nested_onremove_handler() {
        let src = r#"
            const values = _value.map((item, index) => (
              <Pill
                onRemove={() => {
                  setValue(_value.filter((i) => item !== i));
                }}
              />
            ));
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // Regression for #7325 (vuetify VDataTableGroupHeaderRow): a `.filter()` in
    // the map callback whose predicate ignores the iteration binding `column` is
    // loop-invariant (same result every iteration), not a correlated per-element
    // lookup — must not flag.
    #[test]
    fn allows_loop_invariant_filter_independent_of_map_param() {
        let src = r#"columns.map(column => {
            if (column.key === 'x') {
                const r = rows.filter(x => x.selectable);
                return r;
            }
        });"#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // True positive: a `.filter()` whose predicate reads the iteration binding
    // (`x.type === i.type`) is a correlated per-element lookup — must flag.
    #[test]
    fn flags_filter_predicate_correlated_with_map_param() {
        let src = "items.map(i => arr.filter(x => x.type === i.type));";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").len(), 1);
    }

    // Regression for #7743: a `.find()` whose receiver is rooted on the iteration
    // binding (`i.children`) scans an element-local collection — a distinct array
    // each iteration, so total work is Σ|i.children|, linear rather than O(n²).
    // The predicate here *does* reference the binding (`c.id === i.id`), so this
    // isolates the receiver-invariance gate in the `.map()` branch: element-local
    // receiver — must not flag.
    #[test]
    fn allows_find_receiver_element_local_with_map_param() {
        let src = "items.map(i => i.children.find(c => c.id === i.id));";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // True positive: a destructured iteration binding still counts — the
    // predicate reads `id` bound by `({ id }) => …`, a correlated per-element
    // lookup — must flag.
    #[test]
    fn flags_find_correlated_with_destructured_map_param() {
        let src = "items.map(({ id }) => others.find(o => o.id === id));";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").len(), 1);
    }

    // The find/filter predicate shadows the iteration binding name with its own
    // parameter, so the `i` it reads is the inner binding, not the map's `i`.
    // The scan is loop-invariant with respect to the map — must not flag.
    #[test]
    fn allows_filter_when_predicate_shadows_map_param() {
        let src = "items.map(i => arr.filter(i => i.ok));";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // Regression for #7743 (sentry sampledProfile): the receiver `stack` is the map
    // iteration element and the predicate `fn` ignores the binding — a per-element
    // transform over the element's own array, Σ|stack| linear. Must not flag.
    #[test]
    fn allows_filter_element_local_receiver_in_map() {
        let src = "arr.map((stack) => stack.filter(fn));";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // Regression for #7743 (sentry charts): a fresh pipeline over the loop element
    // `data` with a constant predicate (`Number.isFinite`). Element-local, the
    // predicate does not reference the loop binding — must not flag.
    #[test]
    fn allows_pipeline_filter_constant_predicate_in_for_of() {
        let src = "for (const {data} of series) { const m = Math.max(...data.map(x => x.v).filter(Number.isFinite)); }";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // Regression for #7743 (sentry events): the receiver `entry.data?.values` is an
    // optional-chain property of the map element `entry`; the predicate ignores
    // `entry`. Element-local receiver — must not flag.
    #[test]
    fn allows_optional_chain_element_local_filter_in_map() {
        let src = "xs.map(entry => entry.data?.values?.filter(t => !!t.name).length || 0);";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // Regression for #7743 (sentry sentrySampledProfile): the receiver `stack` is a
    // per-iteration local declared inside the `for…of` body; the predicate ignores
    // the loop binding `sample`. Element-local receiver — must not flag.
    #[test]
    fn allows_body_local_receiver_filter_in_for_of() {
        let src = "for (const sample of samples) { let stack = f(sample); stack = stack.filter(fr => opt(fr)); }";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // Regression for #7743: even with a predicate correlated on the loop binding
    // (`e.target === child`), a receiver rooted on a per-iteration body-local
    // (`own = child.entries`) scans an element-local collection — linear, must not
    // flag. Exercises the receiver-invariance gate with a correlated predicate.
    #[test]
    fn allows_body_local_receiver_with_correlated_predicate_in_for_of() {
        let src = "for (const child of childrenEls) { const own = child.entries; own.find(e => e.target === child); }";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // True positive (sentry useRefChildrenVisibility): the receiver `entries` is
    // loop-invariant and the predicate keys on the map element `child` — a genuine
    // O(n²) re-scan. Must stay flagged.
    #[test]
    fn flags_invariant_receiver_correlated_with_map_param() {
        let src = "childrenEls.map(child => entries.find(e => e.target === child));";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").len(), 1);
    }

    // True positive: the same invariant-receiver correlated scan inside a `for…of`
    // is still O(n²). Must stay flagged.
    #[test]
    fn flags_invariant_receiver_correlated_in_for_of() {
        let src = "for (const child of childrenEls) { entries.find(e => e.target === child); }";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").len(), 1);
    }
}
