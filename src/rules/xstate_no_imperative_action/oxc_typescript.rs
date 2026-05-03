//! xstate-no-imperative-action OXC backend — flag `send(...)` or `raise(...)`
//! called outside of an action context.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const ACTION_KEYS: &[&str] = &["actions", "entry", "exit"];

/// Walk ancestors looking for an ObjectProperty whose key is one of the
/// action context keys (`actions`, `entry`, `exit`).
fn is_inside_action_context(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::ObjectProperty(prop) = ancestor.kind() {
            let key_name = match &prop.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => continue,
            };
            if ACTION_KEYS.contains(&key_name) {
                return true;
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let oxc_ast::ast::Expression::Identifier(ident) = &call.callee else {
            return;
        };
        let name = ident.name.as_str();
        if name != "send" && name != "raise" {
            return;
        }

        if is_inside_action_context(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{name}(...)` must be called inside an action (e.g. `actions: [{name}(...)]`), not imperatively."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
