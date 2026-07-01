//! no-inconsistent-returns OxcCheck backend — flag functions that mix
//! `return expr;` with bare `return;`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, peel_parens, return_type_admits_void_or_undefined};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (body, span_start, return_type) = match node.kind() {
            AstKind::Function(f) => {
                let Some(ref body) = f.body else { return };
                (body, f.span().start, f.return_type.as_deref())
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                // Only block-body arrows can have return statements.
                if arrow.expression {
                    return;
                }
                (&arrow.body, arrow.span().start, arrow.return_type.as_deref())
            }
            _ => return,
        };

        let nodes = semantic.nodes();
        let node_id = node.id();

        let mut has_value = false;
        let mut has_bare = false;

        // Walk all nodes in the semantic tree looking for ReturnStatements
        // that belong directly to this function (not nested functions).
        for child in nodes.iter() {
            let AstKind::ReturnStatement(ret) = child.kind() else {
                continue;
            };
            // Check that this return belongs to our function by walking up
            // to find the nearest function boundary.
            if nearest_function_ancestor(child.id(), nodes) != Some(node_id) {
                continue;
            }
            if ret.argument.is_some() {
                has_value = true;
            } else {
                has_bare = true;
            }
            if has_value && has_bare {
                break;
            }
        }

        // A React effect callback has type `() => void | (() => void)`: a bare
        // `return;` ("no cleanup") and `return () => {…}` ("here is the cleanup")
        // are both valid and intentional. Not an inconsistency.
        if has_value && has_bare && is_effect_callback(node_id, nodes) {
            return;
        }

        // An explicit `: void` / `: undefined` / `: any` / `: T | void` /
        // `: T | undefined` return type admits both a bare `return;` (yields
        // `undefined`) and a value return (e.g. a void tail-call, the `undefined`
        // arm of the declared union, or the `JSON.parse` reviver idiom where a
        // bare `return;` drops a key under a `: any` contract). That is the
        // canonical idiom, not an inconsistency.
        if has_value && has_bare && return_type_admits_void_or_undefined(return_type) {
            return;
        }

        // A function whose value paths return JSX is a React component: a bare
        // `return;` there renders nothing (the React-18 equivalent of
        // `return null`). Mixing it with JSX returns is the canonical
        // "render nothing" guard, not an inconsistency.
        if has_value && has_bare && returns_jsx(node_id, nodes) {
            return;
        }

        if has_value && has_bare {
            let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Function has inconsistent returns — some paths return a value, others return nothing.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        let _ = body;
    }
}

/// True when the function `node_id` is the callback passed directly to a React
/// effect hook (`useEffect` / `useLayoutEffect` / `useInsertionEffect`), whose
/// return type is `void | (() => void)`.
fn is_effect_callback(node_id: oxc_semantic::NodeId, nodes: &oxc_semantic::AstNodes) -> bool {
    let parent = nodes.parent_id(node_id);
    if parent == node_id {
        return false;
    }
    let AstKind::CallExpression(call) = nodes.get_node(parent).kind() else {
        return false;
    };
    let callee_name = match &call.callee {
        Expression::Identifier(id) => id.name.as_str(),
        Expression::StaticMemberExpression(m) => m.property.name.as_str(),
        _ => return false,
    };
    matches!(
        callee_name,
        "useEffect" | "useLayoutEffect" | "useInsertionEffect"
    )
}

/// True when a value-returning `ReturnStatement` belonging directly to the
/// function `node_id` (not a nested function) returns a JSX element or fragment.
/// Parentheses are peeled so the multi-line `return ( <Foo/> );` form matches.
/// Uses the same function-scoping as the `has_value` / `has_bare` collection, so
/// JSX in a nested closure or inner component does not leak into this check.
fn returns_jsx(node_id: oxc_semantic::NodeId, nodes: &oxc_semantic::AstNodes) -> bool {
    nodes.iter().any(|child| {
        let AstKind::ReturnStatement(ret) = child.kind() else {
            return false;
        };
        let Some(arg) = ret.argument.as_ref() else {
            return false;
        };
        matches!(
            peel_parens(arg),
            Expression::JSXElement(_) | Expression::JSXFragment(_)
        ) && nearest_function_ancestor(child.id(), nodes) == Some(node_id)
    })
}

/// Walk up from `id` to find the nearest Function or ArrowFunctionExpression
/// ancestor. Returns `Some(node_id)` of that ancestor or `None` if we hit the
/// program root.
fn nearest_function_ancestor(
    id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> Option<oxc_semantic::NodeId> {
    let mut current = nodes.parent_id(id);
    loop {
        if current == nodes.parent_id(current) {
            // Reached root without finding a function.
            return None;
        }
        match nodes.get_node(current).kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return Some(current),
            _ => {
                current = nodes.parent_id(current);
            }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_on_tsx(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_mixed_returns() {
        let code = r#"
function foo(x) {
    if (x) {
        return 42;
    }
    return;
}
"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn allows_consistent_value_returns() {
        let code = r#"
function foo(x) {
    if (x) {
        return 42;
    }
    return 0;
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_consistent_bare_returns() {
        let code = r#"
function foo(x) {
    if (x) {
        return;
    }
    return;
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn flags_async_function() {
        let code = r#"
async function fetchData(url) {
    if (!url) {
        return;
    }
    return fetch(url);
}
"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn does_not_attribute_arrow_returns_to_outer_fn() {
        let code = r#"
function outer() {
    const cb = (x) => {
        if (x === 0) {
            return;
        }
        console.log(x);
    };
    return 1;
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn flags_arrow_with_inconsistent_returns() {
        let code = r#"
const f = (x) => {
    if (x === 0) {
        return;
    }
    return x + 1;
};
"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn allows_use_effect_optional_cleanup() {
        // Regression for issue #578: `return;` (no cleanup) and `return () => {}`
        // (cleanup) are both valid in a useEffect callback.
        let code = r#"
useEffect(() => {
    if (liveName === undefined) {
        return;
    }
    setLiveRouteTitle(pathname, liveName);
    return () => {
        clearLiveRouteTitle(pathname);
    };
}, [liveName, pathname]);
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_use_layout_effect_optional_cleanup() {
        let code = r#"
useLayoutEffect(() => {
    if (!ref.current) return;
    return () => cleanup();
}, []);
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn still_flags_non_effect_callback_with_mixed_returns() {
        // A plain callback (not an effect hook) with mixed returns still flags.
        let code = r#"
useMemo(() => {
    if (x) return;
    return compute();
}, [x]);
"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn allows_union_with_undefined_return_type() {
        // Regression for issue #3948: `: PluginFilter | undefined` — bare `return;`
        // is the `undefined` arm of the declared union.
        let code = r#"
function createFilter(exclude, include): PluginFilter | undefined {
    if (!exclude && !include) {
        return;
    }
    return input => input;
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_void_return_type_with_void_tail_call() {
        // Regression for issue #3948: `: void` — `return voidCall();` is a void
        // tail-call, mixed with bare `return;` returns void on every path.
        let code = r#"
function deoptimizePath(path): void {
    if (path.lost) {
        return;
    }
    return voidCall();
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_arrow_with_union_undefined_return_type() {
        // Regression for issue #3948: block-body arrow annotated `: string | undefined`.
        let code = r#"
const f = (c, s): string | undefined => {
    if (c) return;
    return s;
};
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_async_arrow_promise_union_undefined_return_type() {
        // Regression for issue #6558 (egoist/tsup src/utils.ts:398): an async
        // arrow declared `: Promise<T | undefined>` mixing a bare `return;` with
        // value returns — the bare `return` is the `undefined` arm of the
        // promise's type argument.
        let code = r#"
const resolve = async (x): Promise<NormalizedConfig | undefined> => {
    if (x == null) {
        return;
    }
    return { entry: {} };
};
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_async_function_promise_void_return_type() {
        // `: Promise<T | void>` — the bare `return;` is the `void` arm of the
        // promise's type argument.
        let code = r#"
async function run(x): Promise<Result | void> {
    if (!x) {
        return;
    }
    return compute(x);
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn still_flags_async_promise_without_undefined_arm() {
        // `: Promise<T>` with no `| undefined` / `| void` in the type argument:
        // a bare `return;` yields `undefined`, not the declared awaited type.
        // Genuine inconsistency, must still flag.
        let code = r#"
async function load(x): Promise<NormalizedConfig> {
    if (!x) {
        return;
    }
    return { entry: {} };
}
"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn allows_any_return_type_with_mixed_returns() {
        // Regression for issue #6440 (unjs/destr src/index.ts): the canonical
        // `JSON.parse` reviver idiom `(key, value): any` returns `undefined`
        // (bare `return;`) to drop a key and a value otherwise. `any` includes
        // `undefined`, so this is not an inconsistency.
        let code = r#"
function jsonParseTransform(key: string, value: any): any {
    if (key === "__proto__") {
        return;
    }
    return value;
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_union_with_any_return_type() {
        // `: any | string` — the `any` member admits a bare `return;`.
        let code = r#"
function f(x): any | string {
    if (x) return;
    return "v";
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn still_flags_unannotated_mixed_returns() {
        // No return-type annotation: genuine inconsistency, must still flag.
        let code = r#"
function h(x) {
    if (x) return 1;
    return;
}
"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn still_flags_non_void_annotated_mixed_returns() {
        // `: number` — bare `return;` yields `undefined`, not the declared type.
        // Genuine inconsistency, must still flag.
        let code = r#"
function n(x): number {
    if (x) return 1;
    return;
}
"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn allows_component_bare_return_with_jsx_return() {
        // Regression for issue #7076 (better-auth docs/components/mdx/mermaid.tsx):
        // a React component's bare `return;` renders nothing (equivalent to
        // `return null`), mixed with a JSX value return.
        let code = r#"
export function Mermaid({ chart }) {
    if (!mounted) return;
    return <MermaidContent chart={chart} />;
}
"#;
        assert!(run_on_tsx(code).is_empty());
    }

    #[test]
    fn allows_arrow_component_bare_return_with_jsx_return() {
        let code = r#"
const C = () => {
    if (!x) return;
    return <Foo />;
};
"#;
        assert!(run_on_tsx(code).is_empty());
    }

    #[test]
    fn allows_component_bare_return_with_jsx_fragment_return() {
        let code = r#"
function C() {
    if (!x) return;
    return <>hi</>;
}
"#;
        assert!(run_on_tsx(code).is_empty());
    }

    #[test]
    fn allows_component_bare_return_with_parenthesized_jsx_return() {
        // The idiomatic multi-line component return wraps its JSX in parens; the
        // parser preserves the `ParenthesizedExpression`, so it must be peeled.
        let code = r#"
function Mermaid({ chart }) {
    if (!mounted) return;
    return (
        <MermaidContent chart={chart} />
    );
}
"#;
        assert!(run_on_tsx(code).is_empty());
    }

    #[test]
    fn still_flags_non_jsx_mixed_returns_in_tsx() {
        // A non-JSX function mixing a value return with a bare `return;` is the
        // rule's genuine target — still flags even in a `.tsx` file.
        let code = r#"
function h(x) {
    if (x) return 5;
    return;
}
"#;
        assert_eq!(run_on_tsx(code).len(), 1);
    }

    #[test]
    fn does_not_attribute_inner_component_jsx_to_outer() {
        // The outer function returns a plain value and has a bare `return;`; the
        // JSX belongs to a nested component. The JSX must not exempt the outer.
        let code = r#"
function outer(x) {
    const Inner = () => {
        return <Foo />;
    };
    if (x) return;
    return 5;
}
"#;
        assert_eq!(run_on_tsx(code).len(), 1);
    }

    #[test]
    fn does_not_attribute_method_shorthand_returns_to_outer() {
        let code = r#"
function outer() {
    const obj = {
        foo() {
            if (true) return;
            console.log("ok");
        },
    };
    return 1;
}
"#;
        assert!(run_on(code).is_empty());
    }
}
