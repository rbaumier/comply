//! jsx-handler-names oxc backend — flag JSX event handler props wired to
//! bare identifiers without `handle`, `on`, or `toggle` prefix.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue,
};
use std::sync::Arc;

pub struct Check;

/// True if `name` looks like an event-handler prop: `on` followed by an
/// uppercase letter (e.g. `onClick`, `onSubmit`).
fn is_event_handler_prop(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 3 || &bytes[..2] != b"on" {
        return false;
    }
    bytes[2].is_ascii_uppercase()
}

/// True if the identifier name starts with an accepted handler prefix.
fn has_valid_handler_prefix(name: &str) -> bool {
    let prefixes: [&str; 3] = ["handle", "on", "toggle"];
    prefixes.iter().any(|p| {
        if let Some(rest) = name.strip_prefix(p) {
            rest.as_bytes()
                .first()
                .is_none_or(|b| b.is_ascii_uppercase())
        } else {
            false
        }
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(attr_name) = &attr.name else {
                continue;
            };
            let name_str = attr_name.name.as_str();
            if !is_event_handler_prop(name_str) {
                continue;
            }
            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };
            let Some(expr) = container.expression.as_expression() else {
                continue;
            };
            // Only flag bare identifiers; inline functions, calls, and member
            // expressions are all fine.
            let Expression::Identifier(ident) = expr else {
                continue;
            };
            let ident_name = ident.name.as_str();
            if has_valid_handler_prefix(ident_name) {
                continue;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, ident.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Handler `{ident_name}` passed to `{name_str}` should be named `handle*`, `on*`, or `toggle*`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
