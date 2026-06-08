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

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_get_with_body_schema() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => 'ok', { body: t.Object({ a: t.String() }) });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_head_with_body_model_ref() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().head('/x', () => 'ok', { body: 'model.x' });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_get_with_query_only() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => 'ok', { query: t.Object({ q: t.String() }) });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/x', () => 'ok', { body: t.Object({}) });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }


    #[test]
    fn flags_get_with_typed_reference_body() {
        let src = "import { Elysia } from 'elysia';\nimport { UserSchema } from './schemas';\nnew Elysia().get('/x', () => 'ok', { body: UserSchema });";
        assert_eq!(run_on(src).len(), 1);
    }
}
