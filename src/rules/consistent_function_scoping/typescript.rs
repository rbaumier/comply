//! consistent-function-scoping backend — flag nested functions that
//! capture nothing from their enclosing scope, via oxc_semantic.
//!
//! A nested function/arrow is eligible if all of the following hold:
//! - it is not at the top level (there is at least one enclosing
//!   function/arrow between it and the Program),
//! - it is not a class method or an object method shorthand,
//! - it is not written inline as a direct callback argument
//!   (`arr.map((x) => …)`) or as an IIFE,
//! - it does not reference `this` (for non-arrow functions `this` is a
//!   dynamic binding, so we leave those alone).
//!
//! An eligible function is flagged when it closes over no symbol
//! declared in any scope strictly between the global/module scope and
//! the function itself. Captures are detected by walking every symbol
//! declared in an ancestor scope and checking whether any of its
//! resolved references falls inside the candidate function's span.

use oxc_ast::AstKind;
use oxc_semantic::NodeId;
use oxc_span::{GetSpan, Span};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{source_type_for_path, with_semantic};
use crate::rules::backend::CheckCtx;

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_type = source_type_for_path(ctx.path);
        with_semantic(ctx.source, source_type, |semantic| {
            let scoping = semantic.scoping();
            let nodes = semantic.nodes();
            let root_scope = scoping.root_scope_id();
            let mut diagnostics = Vec::new();

            for (node_id, node) in nodes.iter_enumerated() {
                let (func_span, is_arrow, func_name, own_scope) = match node.kind() {
                    AstKind::Function(func) => {
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
                // Non-arrow functions that reference `this` are not safe
                // to hoist: their `this` binding comes from the call
                // site.
                if !is_arrow && references_this_directly(nodes, func_span) {
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
                    path: ctx.path.to_path_buf(),
                    line,
                    column,
                    rule_id: "consistent-function-scoping".into(),
                    message,
                    severity: Severity::Warning,
                    span: None,
                });
            }

            diagnostics
        })
    }
}

/// True when there is at least one enclosing function/arrow between
/// the node and the `Program`. Top-level functions (optionally wrapped
/// in `export`/`export default`) return false.
fn is_nested(nodes: &oxc_semantic::AstNodes, node_id: NodeId) -> bool {
    for kind in nodes.ancestor_kinds(node_id).skip(1) {
        match kind {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return true,
            AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
}

/// Skip methods, callbacks, IIFEs, and decorators — the rule has
/// nothing to say about those.
fn is_skipped_context(nodes: &oxc_semantic::AstNodes, node_id: NodeId) -> bool {
    let parent_id = nodes.parent_id(node_id);
    if parent_id == node_id {
        return true;
    }
    let parent_kind = nodes.kind(parent_id);

    match parent_kind {
        // Class methods, object method shorthand, property definitions
        // that bind a function.
        AstKind::MethodDefinition(_)
        | AstKind::PropertyDefinition(_)
        | AstKind::ObjectProperty(_)
        | AstKind::AccessorProperty(_) => true,
        // Direct callback argument: `arr.map(fn)`, `setTimeout(fn, 0)`.
        AstKind::CallExpression(call) => {
            let node_span = nodes.kind(node_id).span();
            if call.callee.span() == node_span {
                // IIFE: `(function () { … })()` — the function is the
                // callee, not an argument.
                return true;
            }
            call.arguments.iter().any(|arg| arg.span() == node_span)
        }
        AstKind::NewExpression(new_expr) => {
            let node_span = nodes.kind(node_id).span();
            new_expr.arguments.iter().any(|arg| arg.span() == node_span)
        }
        // Parenthesised IIFE: `(function () {})()` wraps the function
        // in a ParenthesizedExpression before the CallExpression.
        AstKind::ParenthesizedExpression(_) => {
            let grandparent_id = nodes.parent_id(parent_id);
            if grandparent_id == parent_id {
                return false;
            }
            matches!(
                nodes.kind(grandparent_id),
                AstKind::CallExpression(_) | AstKind::NewExpression(_)
            )
        }
        _ => false,
    }
}

/// True when the function body (identified by its span, excluding any
/// nested non-arrow function) references `this`. Arrow functions
/// inherit `this` from the enclosing scope so we only care about this
/// check for non-arrow `Function` nodes.
fn references_this_directly(nodes: &oxc_semantic::AstNodes, func_span: Span) -> bool {
    for node in nodes.iter() {
        if !matches!(node.kind(), AstKind::ThisExpression(_)) {
            continue;
        }
        let this_span = node.kind().span();
        if !span_contains(func_span, this_span) {
            continue;
        }
        // Ensure the `this` is not nested in another non-arrow function
        // inside the candidate (that inner function's `this` is its
        // own). Walk ancestors until we hit either the candidate
        // function or another non-arrow function.
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
                AstKind::ArrowFunctionExpression(_) => {}
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

/// Walks all symbols whose declaration scope sits strictly between the
/// candidate's scope and the root scope. Returns true as soon as any
/// resolved reference to one of those symbols appears inside
/// `func_span`.
fn captures_outer_symbol(
    scoping: &oxc_semantic::Scoping,
    nodes: &oxc_semantic::AstNodes,
    func_scope: oxc_semantic::ScopeId,
    root_scope: oxc_semantic::ScopeId,
    func_span: Span,
) -> bool {
    // Collect the set of ancestor scope ids (excluding the function's
    // own scope and the root scope).
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

fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_nested_function_without_capture() {
        let src = "function outer() {\n  const x = 1;\n  function helper(a: number) { return a * 2; }\n  return helper(x);\n}\n";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("helper"));
    }

    #[test]
    fn allows_nested_function_with_capture() {
        let src = "function outer() {\n  const multiplier = 2;\n  function helper(a: number) { return a * multiplier; }\n  return helper(3);\n}\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_top_level_function() {
        assert!(run_on("function topLevel() { return 1; }\n").is_empty());
    }

    #[test]
    fn skips_iife() {
        assert!(run_on("(function() { return 1; })();\n").is_empty());
    }

    #[test]
    fn skips_inline_arrow_callback() {
        assert!(run_on("const arr = [1,2,3]; arr.map((x) => x * 2);\n").is_empty());
    }

    #[test]
    fn skips_inline_function_callback() {
        assert!(run_on(
            "const arr = [1,2,3]; arr.forEach(function(x) { console.log(x); });\n"
        )
        .is_empty());
    }

    #[test]
    fn skips_class_method() {
        assert!(run_on("class Foo { bar() { return 1; } }\n").is_empty());
    }

    #[test]
    fn skips_function_using_this() {
        let src = "function outer() {\n  function inner() { return this.value; }\n  return inner;\n}\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_nested_arrow_without_capture() {
        let src = "function outer() {\n  const helper = (a: number) => a * 2;\n  return helper(3);\n}\n";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_arrow_capturing_outer_param() {
        let src = "function outer(factor: number) {\n  const helper = (a: number) => a * factor;\n  return helper(3);\n}\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_deeply_nested_function_without_capture() {
        let src = "function a() {\n  function b() {\n    function c() { return 42; }\n    return c();\n  }\n  return b();\n}\n";
        // `c` captures nothing and `b` captures nothing — both flagged.
        let d = run_on(src);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn allows_function_capturing_transitive_outer() {
        // Both `b` and `c` are considered to capture `x`: `c` directly,
        // and `b` because `c`'s reference to `x` is lexically inside
        // `b`'s body.
        let src = "function a() {\n  const x = 1;\n  function b() {\n    function c() { return x; }\n    return c();\n  }\n  return b();\n}\n";
        assert!(run_on(src).is_empty());
    }
}
