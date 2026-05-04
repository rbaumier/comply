//! tanstack-query-stable-client OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const STABLE_WRAPPERS: &[&str] = &["useState", "useRef", "useMemo", "useCallback"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["QueryClient"])
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
        let Expression::Identifier(ctor) = &new_expr.callee else {
            return;
        };
        if ctor.name != "QueryClient" {
            return;
        }

        // Must be inside a PascalCase component.
        let Some(component_id) = enclosing_component(node, semantic) else {
            return;
        };

        // Must NOT be inside a stable wrapper call.
        if inside_stable_wrapper(node, component_id, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new QueryClient()` inside a component — hoist to module scope or wrap in `useState(() => new QueryClient())`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Walk up to find a PascalCase component function. Returns its NodeId.
fn enclosing_component(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::NodeId> {
    let mut current = node.id();
    loop {
        let parent_id = semantic.nodes().parent_id(current);
        if parent_id == current {
            return None;
        }
        current = parent_id;
        let parent = semantic.nodes().get_node(current);
        match parent.kind() {
            AstKind::Function(func) => {
                let name = func.id.as_ref().map(|id| id.name.as_str()).unwrap_or("");
                if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                    return Some(current);
                }
            }
            AstKind::ArrowFunctionExpression(_) => {
                // Check if parent is a variable_declarator with PascalCase name.
                let gp_id = semantic.nodes().parent_id(current);
                if gp_id == current {
                    continue;
                }
                let gp = semantic.nodes().get_node(gp_id);
                if let AstKind::VariableDeclarator(decl) = gp.kind()
                    && let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &decl.id
                        && id.name.starts_with(|c: char| c.is_ascii_uppercase()) {
                            return Some(current);
                        }
            }
            _ => {}
        }
    }
}

/// Check if any ancestor between `node` and `boundary_id` is a call to
/// a stable wrapper hook.
fn inside_stable_wrapper(
    node: &oxc_semantic::AstNode,
    boundary_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let mut current = node.id();
    loop {
        let parent_id = semantic.nodes().parent_id(current);
        if parent_id == current || parent_id == boundary_id {
            return false;
        }
        current = parent_id;
        let parent = semantic.nodes().get_node(current);
        if let AstKind::CallExpression(call) = parent.kind() {
            let name = match &call.callee {
                Expression::Identifier(id) => Some(id.name.as_str()),
                Expression::StaticMemberExpression(mem) => {
                    if let Expression::Identifier(obj) = &mem.object {
                        if obj.name == "React" {
                            Some(mem.property.name.as_str())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(n) = name
                && STABLE_WRAPPERS.contains(&n) {
                    return true;
                }
        }
    }
}
