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
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message,
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

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
        // JSX prop callbacks — render-prop helpers (Base UI Combobox,
        // RHF Controller render, etc.) stay co-located with the JSX
        // they produce even when they don't close over any local.
        // Hoisting them out moves the render logic away from the
        // structure that consumes it, which hurts readability more
        // than a missing closure-capture indicator helps.
        AstKind::JSXExpressionContainer(_) => true,
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
                AstKind::CallExpression(_) | AstKind::NewExpression(_)
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
}
