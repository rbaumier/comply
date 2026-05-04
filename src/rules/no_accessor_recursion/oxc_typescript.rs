//! no-accessor-recursion OXC backend — flag getters/setters that recurse
//! on `this`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

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

        // Object must be `this`.
        if !matches!(&member.object, Expression::ThisExpression(_)) {
            return;
        }

        let prop_name = member.property.name.as_str();

        // Find the enclosing getter/setter.
        let Some((accessor_kind, accessor_name)) = find_accessor_ancestor(node, semantic) else {
            return;
        };

        // The property being accessed must match the accessor name.
        if prop_name != accessor_name {
            return;
        }

        if accessor_kind == "get" {
            // A getter reading its own property is recursion — unless it's
            // being written to.
            let is_write_target = is_assignment_target(node, semantic);
            if !is_write_target {
                let (line, column) = byte_offset_to_line_col(ctx.source, member.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-accessor-recursion".into(),
                    message: "Recursive access to `this` within getter causes infinite recursion.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        } else if accessor_kind == "set" {
            // A setter writing to its own property is recursion.
            let is_write_target = is_assignment_target(node, semantic);
            if is_write_target {
                let (line, column) = byte_offset_to_line_col(ctx.source, member.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-accessor-recursion".into(),
                    message: "Recursive access to `this` within setter causes infinite recursion.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

/// Walk up ancestors to find the enclosing getter/setter MethodDefinition.
/// Returns (kind, property_name). Stops at class boundaries and non-arrow
/// function boundaries (regular functions define their own `this`).
fn find_accessor_ancestor<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<(&'static str, String)> {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::MethodDefinition(method) => {
                let kind = match method.kind {
                    MethodDefinitionKind::Get => "get",
                    MethodDefinitionKind::Set => "set",
                    _ => return None, // Regular method — not an accessor.
                };
                let name = property_key_name(&method.key)?;
                return Some((kind, name));
            }
            // Don't cross class boundaries.
            AstKind::Class(_) => return None,
            // Arrow functions inherit `this` — traverse through them.
            AstKind::ArrowFunctionExpression(_) => continue,
            // Regular functions define their own `this` — stop.
            AstKind::Function(_) => return None,
            _ => {}
        }
    }
    None
}

fn property_key_name(key: &PropertyKey) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
        PropertyKey::PrivateIdentifier(id) => Some(id.name.to_string()),
        PropertyKey::StringLiteral(s) => Some(s.value.to_string()),
        _ => None,
    }
}

/// Check if this member expression is on the left side of an assignment or
/// the argument of an update expression.
fn is_assignment_target<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    match parent.kind() {
        AstKind::AssignmentExpression(assign) => {
            let left_span = assign.left.span();
            let AstKind::StaticMemberExpression(member) = node.kind() else {
                return false;
            };
            left_span.start == member.span.start && left_span.end == member.span.end
        }
        AstKind::UpdateExpression(_) => true,
        _ => false,
    }
}
