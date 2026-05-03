//! OxcCheck backend for elysia-no-body-on-get.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

const BODYLESS_METHODS: &[&str] = &["get", "head"];

pub struct Check;

/// Walk call arguments looking for an object literal with a `body:` key
/// whose value is non-empty.
fn options_has_body_key(call: &oxc_ast::ast::CallExpression, source: &str) -> bool {
    for arg in &call.arguments {
        let Some(Expression::ObjectExpression(obj)) = arg.as_expression() else {
            continue;
        };
        for prop in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
            let PropertyKey::StaticIdentifier(key) = &p.key else { continue };
            if key.name.as_str() != "body" {
                continue;
            }
            let value_text = &source[p.value.span().start as usize..p.value.span().end as usize];
            if !value_text.trim().is_empty() {
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
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop = member.property.name.as_str();
        if !BODYLESS_METHODS.contains(&prop) {
            return;
        }

        if !options_has_body_key(call, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.get()` and `.head()` cannot carry a request body — move validation to `query:` or use `.post()`.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
