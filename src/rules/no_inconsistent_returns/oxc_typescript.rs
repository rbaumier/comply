//! no-inconsistent-returns OxcCheck backend — flag functions that mix
//! `return expr;` with bare `return;`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
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
        let (body, span_start) = match node.kind() {
            AstKind::Function(f) => {
                let Some(ref body) = f.body else { return };
                (body, f.span().start)
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                // Only block-body arrows can have return statements.
                if arrow.expression {
                    return;
                }
                (&arrow.body, arrow.span().start)
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
