//! react-no-prevent-default OxcCheck backend.
//!
//! Flags `event.preventDefault()` inside JSX handlers for passive event
//! attributes (`onScroll`, `onWheel`, `onTouchStart`, `onTouchMove`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const PASSIVE_HANDLERS: &[&str] = &["onScroll", "onWheel", "onTouchStart", "onTouchMove"];

pub struct Check;

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

        // Must be `<expr>.preventDefault()`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "preventDefault" {
            return;
        }

        // Walk ancestors to find enclosing JSXAttribute
        let nodes = semantic.nodes();
        let mut current_id = node.id();
        loop {
            let parent_id = nodes.parent_id(current_id);
            if parent_id == current_id {
                return;
            }
            let parent = nodes.get_node(parent_id);
            match parent.kind() {
                AstKind::JSXAttribute(attr) => {
                    let oxc_ast::ast::JSXAttributeName::Identifier(name_ident) = &attr.name else {
                        return;
                    };
                    let attr_name = name_ident.name.as_str();
                    if !PASSIVE_HANDLERS.contains(&attr_name) {
                        return;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span().start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`preventDefault()` inside `{attr_name}` is a no-op — React attaches this listener as passive."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
                AstKind::Program(_) => return,
                _ => {
                    current_id = parent_id;
                }
            }
        }
    }
}
