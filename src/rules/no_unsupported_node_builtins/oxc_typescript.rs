//! no-unsupported-node-builtins oxc backend — compare each Node.js API usage
//! against the minimum Node version declared in the nearest `package.json`'s
//! `engines.node` field.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use super::{lookup_global, lookup_instance_method, lookup_static_method, min_node_major};

pub struct Check;

/// True if the identifier is in a declaration position (variable name, param
/// name, function/class name). Prevents "shim" declarations from tripping
/// the rule on themselves.
fn is_declaration_name<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let parent_id = semantic.nodes().parent_id(node.id());
    if node.id() == parent_id {
        return false;
    }
    let parent = semantic.nodes().get_node(parent_id);
    matches!(
        parent.kind(),
        AstKind::VariableDeclarator(_)
            | AstKind::Function(_)
            | AstKind::Class(_)
            | AstKind::MethodDefinition(_)
            | AstKind::FormalParameter(_)
            | AstKind::FormalParameters(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::BindingRestElement(_)
            | AstKind::LabeledStatement(_)
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let Some(min_version) = min_node_major(ctx) else {
            return Vec::new();
        };

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::IdentifierReference(ident) => {
                    if is_declaration_name(node, semantic) {
                        continue;
                    }
                    let text = ident.name.as_str();
                    if let Some(required) = lookup_global(text).filter(|&r| r > min_version) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, ident.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{text}` is not available in Node.js {min_version}; requires Node.js {required} or later."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                AstKind::StaticMemberExpression(mem) => {
                    let prop_text = mem.property.name.as_str();

                    // Static method on `Object` / `Array`.
                    if let oxc_ast::ast::Expression::Identifier(obj) = &mem.object {
                        let obj_text = obj.name.as_str();
                        if let Some(required) =
                            lookup_static_method(obj_text, prop_text).filter(|&r| r > min_version)
                        {
                            let (line, column) =
                                byte_offset_to_line_col(ctx.source, mem.span.start as usize);
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line,
                                column,
                                rule_id: super::META.id.into(),
                                message: format!(
                                    "`{obj_text}.{prop_text}` is not available in Node.js {min_version}; requires Node.js {required} or later."
                                ),
                                severity: Severity::Warning,
                                span: None,
                            });
                            continue;
                        }
                    }

                    // Instance method — flagged regardless of receiver shape.
                    if let Some(required) =
                        lookup_instance_method(prop_text).filter(|&r| r > min_version)
                    {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, mem.property.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`.{prop_text}()` is not available in Node.js {min_version}; requires Node.js {required} or later."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}
