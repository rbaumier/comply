//! react-no-this-in-sfc OxcCheck backend.
//!
//! Detects `this.` inside functional components. Functional components
//! use hooks, not `this.state` / `this.props`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Check if a function/arrow subtree contains JSX.
fn subtree_has_jsx(semantic: &oxc_semantic::Semantic<'_>, fn_node_id: oxc_semantic::NodeId) -> bool {
    let nodes = semantic.nodes();
    // Walk all descendants looking for JSX
    for node in nodes.iter() {
        // Check if this node is a descendant of our function
        let mut current = node.id();
        let mut is_descendant = false;
        loop {
            if current == fn_node_id {
                is_descendant = true;
                break;
            }
            let parent = nodes.parent_id(current);
            if parent == current {
                break;
            }
            current = parent;
        }
        if !is_descendant {
            continue;
        }
        match node.kind() {
            AstKind::JSXElement(_) | AstKind::JSXOpeningElement(_) | AstKind::JSXFragment(_) => {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Check if a node is inside a class body.
fn is_inside_class(node_id: oxc_semantic::NodeId, semantic: &oxc_semantic::Semantic<'_>) -> bool {
    let nodes = semantic.nodes();
    let mut current = node_id;
    loop {
        let parent_id = nodes.parent_id(current);
        if parent_id == current {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        if matches!(parent.kind(), AstKind::Class(_)) {
            return true;
        }
        current = parent_id;
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StaticMemberExpression(member) = node.kind() else {
            return;
        };

        // Object must be `this`
        let Expression::ThisExpression(_) = &member.object else {
            return;
        };

        // Walk up to find enclosing function/arrow
        let nodes = semantic.nodes();
        let mut current_id = node.id();
        loop {
            let parent_id = nodes.parent_id(current_id);
            if parent_id == current_id {
                return;
            }
            let parent = nodes.get_node(parent_id);
            match parent.kind() {
                AstKind::Function(f) => {
                    // Must be a PascalCase named function
                    let Some(id) = &f.id else {
                        return;
                    };
                    let name = id.name.as_str();
                    if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
                        return;
                    }
                    // Must not be inside a class
                    if is_inside_class(parent_id, semantic) {
                        return;
                    }
                    // Must contain JSX
                    if !subtree_has_jsx(semantic, parent_id) {
                        return;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, member.span().start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "react-no-this-in-sfc".into(),
                        message: "`this` has no meaning in a functional component. \
                                  Use hooks instead."
                            .into(),
                        severity: Severity::Error,
                        span: None,
                    });
                    return;
                }
                AstKind::ArrowFunctionExpression(_) => {
                    // Arrow function — check if parent is a variable declarator with PascalCase
                    let arrow_parent_id = nodes.parent_id(parent_id);
                    if arrow_parent_id == parent_id {
                        return;
                    }
                    let arrow_parent = nodes.get_node(arrow_parent_id);
                    let AstKind::VariableDeclarator(decl) = arrow_parent.kind() else {
                        return;
                    };
                    let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) =
                        &decl.id
                    else {
                        return;
                    };
                    let name = ident.name.as_str();
                    if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
                        return;
                    }
                    // Must not be inside a class
                    if is_inside_class(parent_id, semantic) {
                        return;
                    }
                    // Must contain JSX
                    if !subtree_has_jsx(semantic, parent_id) {
                        return;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, member.span().start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "react-no-this-in-sfc".into(),
                        message: "`this` has no meaning in a functional component. \
                                  Use hooks instead."
                            .into(),
                        severity: Severity::Error,
                        span: None,
                    });
                    return;
                }
                _ => {
                    current_id = parent_id;
                }
            }
        }
    }
}
