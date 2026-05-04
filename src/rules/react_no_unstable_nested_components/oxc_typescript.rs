//! OXC backend for react-no-unstable-nested-components.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BindingPattern;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_component_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

fn subtree_has_jsx(node_span: oxc_span::Span, semantic: &oxc_semantic::Semantic) -> bool {
    for n in semantic.nodes().iter() {
        let s = n.kind().span();
        if s.start < node_span.start || s.end > node_span.end {
            continue;
        }
        match n.kind() {
            AstKind::JSXOpeningElement(_) | AstKind::JSXFragment(_) => return true,
            _ => {}
        }
    }
    false
}

/// Get the component name for a node, if it looks like a component.
fn get_component_name_from_kind<'a>(
    kind: &AstKind<'a>,
    parent_kind: &AstKind<'a>,
) -> Option<&'a str> {
    match kind {
        AstKind::Function(func) => {
            let id = func.id.as_ref()?;
            let name = id.name.as_str();
            if is_component_name(name) { Some(name) } else { None }
        }
        AstKind::ArrowFunctionExpression(_) => {
            let AstKind::VariableDeclarator(decl) = parent_kind else {
                return None;
            };
            let BindingPattern::BindingIdentifier(id) = &decl.id else {
                return None;
            };
            let name = id.name.as_str();
            if is_component_name(name) { Some(name) } else { None }
        }
        _ => None,
    }
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
        let parent = semantic.nodes().parent_node(node.id());
        // Must be a component (PascalCase name + has JSX)
        if get_component_name_from_kind(&node.kind(), &parent.kind()).is_none() {
            return;
        }
        let node_span = node.kind().span();
        if !subtree_has_jsx(node_span, semantic) {
            return;
        }

        let is_arrow = matches!(node.kind(), AstKind::ArrowFunctionExpression(_));

        // Check if nested inside another component
        for ancestor in semantic.nodes().ancestors(node.id()).skip(1) {
            match ancestor.kind() {
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                    let anc_parent = semantic.nodes().parent_node(ancestor.id());
                    if get_component_name_from_kind(&ancestor.kind(), &anc_parent.kind()).is_some()
                        && subtree_has_jsx(ancestor.kind().span(), semantic)
                    {
                        // Report at the variable_declarator for arrows
                        let report_span = if is_arrow {
                            parent.kind().span()
                        } else {
                            node_span
                        };

                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, report_span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "react-no-unstable-nested-components".into(),
                            message: "Do not define components during render. React will \
                                      see a new component type on every render and destroy \
                                      the entire subtree's DOM and state. Move it outside \
                                      the parent component."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                        return;
                    }
                }
                // Stop at class or module level
                AstKind::Class(_) | AstKind::Program(_) => return,
                _ => {}
            }
        }
    }
}
