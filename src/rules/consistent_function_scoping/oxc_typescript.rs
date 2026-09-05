//! consistent-function-scoping OXC backend — flag nested functions that
//! capture nothing from their enclosing scope.

use oxc_ast::AstKind;
use oxc_semantic::NodeId;
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir {
            return Vec::new();
        }
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let root_scope = scoping.root_scope_id();
        let mut diagnostics = Vec::new();

        for (node_id, node) in nodes.iter_enumerated() {
            let (func_span, is_arrow, func_name, own_scope) = match node.kind() {
                AstKind::Function(func) => {
                    // A bodyless function is a TypeScript overload signature or
                    // an ambient declaration — pure type surface with no runtime
                    // body, references, or captures. It cannot be hoisted: an
                    // overload signature must immediately precede its
                    // implementation in the same scope. Never a nested helper.
                    if func.body.is_none() {
                        continue;
                    }
                    let Some(scope) = func.scope_id.get() else {
                        continue;
                    };
                    (
                        func.span(),
                        false,
                        func.id.as_ref().map(|i| i.name.to_string()),
                        scope,
                    )
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    let Some(scope) = arrow.scope_id.get() else {
                        continue;
                    };
                    (arrow.span(), true, None, scope)
                }
                _ => continue,
            };

            if !is_nested(nodes, node_id) {
                continue;
            }
            if is_skipped_context(nodes, node_id) {
                continue;
            }
            if references_this_directly(nodes, func_span) {
                continue;
            }

            if captures_outer_symbol(scoping, nodes, own_scope, root_scope, func_span) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, func_span.start as usize);
            let message = match &func_name {
                Some(n) => format!(
                    "Function `{n}` does not capture any variable from its parent scope and could be hoisted."
                ),
                None => {
                    "Nested function does not capture any variable from its parent scope and could be hoisted."
                        .to_string()
                }
            };
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message,
                severity: Severity::Error,
                span: None,
            });
        }

        diagnostics
    }
}

/// A function is nested when an enclosing function could host it one level up.
///
/// An immediately invoked enclosing function is not such a host: an IIFE exists
/// to keep its contents out of the surrounding scope — in a classic browser
/// script that scope is the global object — so lifting a helper out of it
/// publishes the helper instead of tidying it. Its body counts as the top level
/// the author asked for.
fn is_nested(nodes: &oxc_semantic::AstNodes, node_id: NodeId) -> bool {
    for (ancestor_id, ancestor) in nodes.ancestors_enumerated(node_id).skip(1) {
        match ancestor.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return !is_immediately_invoked(nodes, ancestor_id);
            }
            AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
}

/// Whether `node_id` sits in the callee slot of a call, through any number of
/// wrapping parentheses — `(function () {}())` and `(() => {})()` alike.
fn is_immediately_invoked(nodes: &oxc_semantic::AstNodes, node_id: NodeId) -> bool {
    let mut callee_id = node_id;
    let mut parent_id = nodes.parent_id(callee_id);
    while parent_id != callee_id
        && matches!(nodes.kind(parent_id), AstKind::ParenthesizedExpression(_))
    {
        callee_id = parent_id;
        parent_id = nodes.parent_id(callee_id);
    }
    matches!(
        nodes.kind(parent_id),
        AstKind::CallExpression(call) if call.callee.span() == nodes.kind(callee_id).span()
    )
}

fn is_skipped_context(nodes: &oxc_semantic::AstNodes, node_id: NodeId) -> bool {
    let parent_id = nodes.parent_id(node_id);
    if parent_id == node_id {
        return true;
    }
    let parent_kind = nodes.kind(parent_id);

    match parent_kind {
        AstKind::MethodDefinition(_)
        | AstKind::PropertyDefinition(_)
        | AstKind::ObjectProperty(_)
        | AstKind::AccessorProperty(_) => true,
        // An element of an array literal is a value in a data position, the
        // same construct as an object-property value above: a lookup table
        // pairing keys with handlers reads as a table, and each row's meaning
        // is its place in it. There is no nearby call site to hoist it away
        // from, so the two spellings of the same table agree.
        AstKind::ArrayExpression(_) => true,
        // JSX prop callbacks — render-prop helpers (Base UI Combobox,
        // RHF Controller render, etc.) stay co-located with the JSX
        // they produce even when they don't close over any local.
        // Hoisting them out moves the render logic away from the
        // structure that consumes it, which hurts readability more
        // than a missing closure-capture indicator helps.
        AstKind::JSXExpressionContainer(_) => true,
        // A default-parameter initializer (`cb = () => {}`) is the value bound
        // to a parameter name, not a hoistable nested helper. An empty no-op
        // default callback is the canonical way to express an optional callback
        // parameter, so it stays nested even when it captures nothing.
        AstKind::FormalParameter(param) => {
            let node_span = nodes.kind(node_id).span();
            param
                .initializer
                .as_ref()
                .is_some_and(|default| default.span() == node_span)
        }
        // A destructuring default (`{ cb = () => {} }`) is the same binding
        // fragment as the parameter default above, in any binding position:
        // parameter list, variable declarator, catch clause, for-of head.
        AstKind::AssignmentPattern(assign) => assign.right.span() == nodes.kind(node_id).span(),
        // A function that is *returned* is the value its parent produces — a
        // useEffect cleanup (`return () => …`), a factory's closure, etc.
        // Hoisting it to module scope separates the produced value from its
        // producer, so it stays nested even when it captures nothing.
        AstKind::ReturnStatement(ret) => {
            let node_span = nodes.kind(node_id).span();
            ret.argument
                .as_ref()
                .is_some_and(|arg| arg.span() == node_span)
        }
        // A concise-body arrow (`() => (input) => {…}`) wraps its
        // implicit-return expression in an `ExpressionStatement` inside a
        // synthetic `FunctionBody`, so a function sitting there is the value the
        // outer arrow produces — the same semantic as the `ReturnStatement`
        // case above. It stays nested even when it captures nothing. A statement
        // inside a *block*-body arrow (`expression == false`) is a genuine
        // nested helper and is not exempt.
        AstKind::ExpressionStatement(expr_stmt) => {
            let node_span = nodes.kind(node_id).span();
            if expr_stmt.expression.span() != node_span {
                return false;
            }
            let body_id = nodes.parent_id(parent_id);
            matches!(nodes.kind(body_id), AstKind::FunctionBody(_))
                && matches!(
                    nodes.kind(nodes.parent_id(body_id)),
                    AstKind::ArrowFunctionExpression(arrow) if arrow.expression
                )
        }
        AstKind::CallExpression(call) => {
            let node_span = nodes.kind(node_id).span();
            if call.callee.span() == node_span {
                return true;
            }
            call.arguments.iter().any(|arg| arg.span() == node_span)
        }
        AstKind::NewExpression(new_expr) => {
            let node_span = nodes.kind(node_id).span();
            new_expr.arguments.iter().any(|arg| arg.span() == node_span)
        }
        AstKind::ParenthesizedExpression(_) => {
            let grandparent_id = nodes.parent_id(parent_id);
            if grandparent_id == parent_id {
                return false;
            }
            matches!(
                nodes.kind(grandparent_id),
                AstKind::CallExpression(_)
                    | AstKind::NewExpression(_)
                    | AstKind::ReturnStatement(_)
            )
        }
        _ => false,
    }
}

fn references_this_directly(nodes: &oxc_semantic::AstNodes, func_span: Span) -> bool {
    for node in nodes.iter() {
        if !matches!(node.kind(), AstKind::ThisExpression(_)) {
            continue;
        }
        let this_span = node.kind().span();
        if !span_contains(func_span, this_span) {
            continue;
        }
        let mut bound_by_candidate = true;
        for kind in nodes.ancestor_kinds(node.id()).skip(1) {
            match kind {
                AstKind::Function(func) => {
                    if func.span() == func_span {
                        break;
                    }
                    bound_by_candidate = false;
                    break;
                }
                // Arrow functions inherit `this` lexically, so an arrow
                // candidate binds a `this` in its body. Stop only at the
                // candidate's own span; inner arrows keep walking outward.
                AstKind::ArrowFunctionExpression(arrow) => {
                    if arrow.span() == func_span {
                        break;
                    }
                }
                AstKind::Program(_) => {
                    bound_by_candidate = false;
                    break;
                }
                _ => {}
            }
        }
        if bound_by_candidate {
            return true;
        }
    }
    false
}

fn captures_outer_symbol(
    scoping: &oxc_semantic::Scoping,
    nodes: &oxc_semantic::AstNodes,
    func_scope: oxc_semantic::ScopeId,
    root_scope: oxc_semantic::ScopeId,
    func_span: Span,
) -> bool {
    let mut ancestor_scopes: Vec<oxc_semantic::ScopeId> = Vec::new();
    let mut cursor = scoping.scope_parent_id(func_scope);
    while let Some(scope) = cursor {
        if scope == root_scope {
            break;
        }
        ancestor_scopes.push(scope);
        cursor = scoping.scope_parent_id(scope);
    }
    if ancestor_scopes.is_empty() {
        return false;
    }

    for symbol_id in scoping.symbol_ids() {
        let symbol_scope = scoping.symbol_scope_id(symbol_id);
        if !ancestor_scopes.contains(&symbol_scope) {
            continue;
        }
        for reference in scoping.get_resolved_references(symbol_id) {
            let ref_span = nodes.kind(reference.node_id()).span();
            if span_contains(func_span, ref_span) {
                return true;
            }
        }
    }
    false
}

fn span_contains(outer: Span, inner: Span) -> bool {
    inner.start >= outer.start && inner.end <= outer.end
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_nested_function_not_capturing() {
        let src = r#"
            function outer() {
                function inner() { return 1; }
                return inner();
            }
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn ignores_jsx_render_prop_callback() {
        // Regression for rbaumier/comply#20 — Base UI / RHF render props.
        let src = r#"
            function MyForm() {
                return <Controller render={({ field }) => <Input {...field} />} />;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_jsx_event_handler() {
        let src = r#"
            function Btn() {
                return <button onClick={() => alert("hi")}>x</button>;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_returned_cleanup_function() {
        // Regression: a useEffect cleanup `return () => …` is the value the
        // effect produces, not a hoistable helper, even with no capture.
        let src = r#"
            function useThing() {
                useEffect(() => {
                    subscribe();
                    return () => unsubscribe();
                }, []);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_arrow_referencing_this() {
        // Regression for rbaumier/comply#1635 — an arrow function captures
        // `this` lexically from the enclosing scope, so it cannot be hoisted.
        let src = r#"
            class FrameTree {
                _setupDragAndDrop(chromeEventHandler) {
                    const emitInputEvent = (event) => this.emit("inputEvent", { type: event.type });
                    helper.addEventListener(chromeEventHandler, "dragstart", emitInputEvent);
                    helper.addEventListener(chromeEventHandler, "dragover", emitInputEvent);
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_arrow_capturing_nothing() {
        // Negative-space guard: an arrow that uses only its own params and
        // captures nothing from the parent scope can still be hoisted.
        let src = r#"
            function outer() {
                const f = (x) => x + 1;
                return f(2);
            }
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn ignores_default_parameter_arrow_initializer() {
        // Regression for rbaumier/comply#3833 — an empty default-callback
        // arrow (`cb = () => {}`) is the value bound to a parameter name, not a
        // hoistable nested helper.
        let src = r#"
            export function animate(property: string, cb = () => {}) {
                cb();
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_default_parameter_arrow_initializer_with_body() {
        // Regression for rbaumier/comply#3833 — a default arrow that uses only
        // its own params is still a parameter default, not a hoistable helper.
        let src = r#"
            export function withDefault(onChange = (v: number) => v * 2) {
                return onChange(3);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_destructured_default_parameter_arrow() {
        // Regression for rbaumier/comply#4770 — an arrow default value inside a
        // destructured object parameter is the value bound to that slot, not a
        // hoistable nested helper (airbnb/visx HeatmapCircle.tsx).
        let src = r#"
            export default function HeatmapCircle({
              colorScale = () => undefined,
              opacityScale = () => 1,
              bins = (column) => column?.bins,
              count = (cell) => cell?.count,
            }) {
              return colorScale();
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_destructured_default_arrow_in_variable_declarator() {
        // Regression for rbaumier/comply#6852 — an arrow default in a
        // destructuring variable declarator is a binding-pattern fragment, the
        // same idiom as a destructured parameter default (trpc jsonl.ts).
        let src = r#"
            export async function jsonlStreamConsumer(opts) {
                const { deserialize = (v) => v } = opts;
                return deserialize(opts.head);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_destructured_default_arrow_in_catch_and_for_of() {
        // The exemption keys on the `AssignmentPattern` right slot, so it holds
        // in every binding position, not only parameter lists and declarators.
        let src = r#"
            function outer(rows, run) {
                for (const { fmt = (v) => v } of rows) { fmt("x"); }
                try { run(); } catch ({ report = (e) => e }) { report(1); }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_nested_helper_declared_beside_destructured_default() {
        // Negative-space guard: the destructuring-default exemption covers only
        // the default slot. A sibling helper in the same body still flags, so
        // the exemption cannot silence the rest of the enclosing body.
        let src = r#"
            function outer(opts) {
                const { cb = () => 1 } = opts;
                function double(x) { return x * 2; }
                return double(cb());
            }
        "#;
        let diags = run(src);
        assert_eq!(diags.len(), 1, "expected only `double` to flag: {diags:?}");
        assert!(diags[0].message.contains("`double`"), "{diags:?}");
    }

    #[test]
    fn flags_helper_declared_inside_destructured_default_body() {
        // Negative-space guard: the exemption reads the direct parent only, so
        // it covers the default expression itself and nothing below it. A
        // helper declared inside that default's body is still hoistable.
        let src = r#"
            function outer(opts) {
                const { cb = () => {
                    function double(x) { return x * 2; }
                    return double(1);
                } } = opts;
                return cb();
            }
        "#;
        let diags = run(src);
        assert_eq!(diags.len(), 1, "expected only `double` to flag: {diags:?}");
        assert!(diags[0].message.contains("`double`"), "{diags:?}");
    }

    #[test]
    fn ignores_nested_overload_signatures() {
        // Regression for rbaumier/comply#6289 — TypeScript function overload
        // signatures (no body) nested in a factory function are pure type
        // surface and cannot be hoisted apart from their implementation
        // (pmndrs/valtio proxySet.ts). Only the bodied implementation is
        // subject to the hoist check.
        let src = r#"
            function proxySet<T>() {
                function intersectionImpl<T, U>(this: Set<T>, other: Set<U>): Set<T & U>
                function intersectionImpl<T>(this: Set<T>, other: Set<T>): Set<T>
                function intersectionImpl<T>(this: Set<T>, other: Set<T>): Set<unknown> {
                    return this.size + other.size;
                }
                return intersectionImpl;
            }
        "#;
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "overload signatures must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn ignores_inner_arrow_as_expression_body_of_outer_arrow() {
        // Regression for rbaumier/comply#6803 — a curried factory's inner arrow
        // IS the expression body (implicit return) of the outer arrow
        // (privatenumber/cleye formats.ts). Hoisting it would separate the
        // produced formatter from its producing factory.
        let src = r#"
            export const integer = () => (input: string): number => {
                const value = Number(input);
                if (!Number.isInteger(value)) {
                    throw new TypeError(`Expected an integer (got: ${input})`);
                }
                return value;
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_bare_arrow_statement_in_block_body() {
        // Negative-space guard exercising the new `ExpressionStatement` arm: a
        // bare arrow expression-statement inside a *block*-body arrow reaches the
        // arm but its owning arrow has `expression == false`, so it is a genuine
        // nested helper, not an implicit return, and still flags. This pins the
        // `arrow.expression` discriminator.
        let src = r#"
            const outer = () => {
                (x: number) => x + 1;
                return 1;
            };
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn ignores_arrows_as_array_literal_elements() {
        // Regression for rbaumier/comply#8172 — a lookup table written as an
        // array of pairs (moment/luxon src/impl/diff.js) is the same construct
        // as the object spelling below, which was already exempt. Both
        // spellings must agree.
        let src = r#"
            export function highOrderDiffs() {
                const differs = [
                    ["years", (a, b) => b.year - a.year],
                    ["quarters", (a, b) => b.quarter - a.quarter + (b.year - a.year) * 4],
                    ["months", (a, b) => b.month - a.month + (b.year - a.year) * 12],
                ];
                return differs;
            }

            export function withObjectProperty() {
                const differs = { years: (a, b) => b.year - a.year };
                return differs;
            }
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_helper_declared_inside_array_element_arrow_body() {
        // Negative-space guard: the array-element exemption covers the element
        // itself and nothing below it. A helper declared inside that element's
        // body is still hoistable.
        let src = r#"
            function outer() {
                const handlers = [
                    (a) => {
                        function double(x) { return x * 2; }
                        return double(a);
                    },
                ];
                return handlers;
            }
        "#;
        let diags = run(src);
        assert_eq!(diags.len(), 1, "expected only `double` to flag: {diags:?}");
        assert!(diags[0].message.contains("`double`"), "{diags:?}");
    }

    #[test]
    fn ignores_helper_inside_arrow_iife_body() {
        // Regression for rbaumier/comply#8172 — a docsify plugin script wraps
        // its contents in an IIFE precisely to keep them out of the global
        // scope, so "hoisting" the plugin would publish it as a global.
        let src = r#"
            (() => {
                const darkThemeTogglePlugin = (hook, vm) => {
                    const TOGGLE_ID = "toggle";
                    hook.mounted(() => {
                        document.body.setAttribute("id", TOGGLE_ID);
                    });
                };

                window.$docsify = window.$docsify || {};
                window.$docsify.plugins = [darkThemeTogglePlugin];
            })();
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn ignores_helper_inside_function_expression_iife_body() {
        // The IIFE exemption keys on the callee slot, so it holds for the
        // `(function () {}())` spelling where the parentheses wrap the call
        // rather than the callee.
        let src = r#"
            (function () {
                function eq(v1, v2) { return v1 === v2; }
                window.eq = eq;
            }());
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_helper_inside_uninvoked_function_expression() {
        // Negative-space guard: a function *expression* is not an IIFE. Only
        // an immediately invoked one opens a module-private scope.
        let src = r#"
            const setup = function () {
                function eq(v1, v2) { return v1 === v2; }
                return eq;
            };
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn flags_helper_nested_two_levels_inside_iife() {
        // Negative-space guard: the IIFE exemption applies to the IIFE's own
        // body only. A helper inside an ordinary function declared in that
        // body is still hoistable to the IIFE scope.
        let src = r#"
            (() => {
                function outer() {
                    function inner() { return 1; }
                    return inner();
                }
                window.outer = outer;
            })();
        "#;
        let diags = run(src);
        assert_eq!(diags.len(), 1, "expected only `inner` to flag: {diags:?}");
        assert!(diags[0].message.contains("`inner`"), "{diags:?}");
    }

    #[test]
    fn flags_arrow_whose_this_belongs_to_nested_function() {
        // An arrow whose only `this` lives inside a nested non-arrow function
        // does not itself capture `this`, so it stays flaggable.
        let src = r#"
            function outer() {
                const f = () => {
                    function g() { return this; }
                    return g();
                };
                return f();
            }
        "#;
        assert!(!run(src).is_empty());
    }
}
