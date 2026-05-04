//! api-branded-id-types OxcCheck backend — flag function parameters named
//! `*Id` / `*_id` typed as bare `string` or `number` in exported functions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, TSType};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::FormalParameter]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::FormalParameter(param) = node.kind() else {
            return;
        };

        // Extract parameter name
        let BindingPattern::BindingIdentifier(ident) = &param.pattern else {
            return;
        };
        let name = ident.name.as_str();
        if !name_looks_like_id(name) {
            return;
        }

        // Check type annotation is bare `string` or `number`
        let Some(type_ann) = &param.type_annotation else {
            return;
        };
        let Some(kind) = bare_primitive_kind(&type_ann.type_annotation) else {
            return;
        };

        // Check if in exported context
        if !is_in_exported_context(node.id(), semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, param.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Parameter `{name}: {kind}` uses a raw primitive — use a branded ID type so unrelated IDs can't be swapped at call sites."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn name_looks_like_id(name: &str) -> bool {
    if name == "id" {
        return true;
    }
    if name.ends_with("_id") && name.len() > 3 {
        return true;
    }
    // camelCase: ends with "Id" and preceded by lowercase
    if name.ends_with("Id") && name.len() > 2 {
        let prev = name.as_bytes()[name.len() - 3];
        if prev.is_ascii_lowercase() {
            return true;
        }
    }
    false
}

fn bare_primitive_kind(ts_type: &TSType<'_>) -> Option<&'static str> {
    match ts_type {
        TSType::TSStringKeyword(_) => Some("string"),
        TSType::TSNumberKeyword(_) => Some("number"),
        _ => None,
    }
}

fn is_in_exported_context(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node_id;

    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);

        match parent.kind() {
            AstKind::Function(_) => {
                // Check if this function is exported
                let gp_id = nodes.parent_id(parent_id);
                if gp_id != parent_id
                    && let AstKind::ExportNamedDeclaration(_) = nodes.get_node(gp_id).kind() {
                        return true;
                    }
                return false;
            }
            AstKind::ArrowFunctionExpression(_) => {
                // Check parent chain: VariableDeclarator -> VariableDeclaration -> ExportNamedDeclaration
                let mut up_id = nodes.parent_id(parent_id);
                loop {
                    if up_id == nodes.parent_id(up_id) {
                        return false;
                    }
                    match nodes.get_node(up_id).kind() {
                        AstKind::VariableDeclarator(_)
                        | AstKind::VariableDeclaration(_) => {
                            up_id = nodes.parent_id(up_id);
                        }
                        AstKind::ExportNamedDeclaration(_) => return true,
                        _ => return false,
                    }
                }
            }
            AstKind::MethodDefinition(_) => {
                // Check if the enclosing class is exported
                let mut up_id = nodes.parent_id(parent_id);
                loop {
                    if up_id == nodes.parent_id(up_id) {
                        return false;
                    }
                    match nodes.get_node(up_id).kind() {
                        AstKind::ClassBody(_) => {
                            up_id = nodes.parent_id(up_id);
                        }
                        AstKind::Class(_) => {
                            let class_parent_id = nodes.parent_id(up_id);
                            if class_parent_id != up_id
                                && let AstKind::ExportNamedDeclaration(_) =
                                    nodes.get_node(class_parent_id).kind()
                                {
                                    return true;
                                }
                            return false;
                        }
                        _ => return false,
                    }
                }
            }
            _ => {}
        }
        current_id = parent_id;
    }
}
