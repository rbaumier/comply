//! elysia-route-missing-body-schema oxc backend — flag `.post/.put/.patch`
//! routes whose handler destructures `body` but options carry no `body:` schema.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const ROUTE_METHODS: &[&str] = &["post", "put", "patch"];

pub struct Check;

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

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        use oxc_ast::ast::Expression;

        // Must be a method call like `app.post(...)`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let prop = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&prop) {
            return;
        }

        // Check the arguments as source text for the handler body pattern.
        let args_start = call.span.start as usize;
        let args_end = call.span.end as usize;
        let args_text = &ctx.source[args_start..args_end];
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

        // Handler destructures `body`.
        let handler_uses_body = norm.contains("({body")
            || norm.contains(",{body")
            || norm.contains("{body,")
            || norm.contains("{body}")
            || norm.contains("{body:");

        if !handler_uses_body {
            return;
        }

        // Check if any argument that is an object literal contains a `body:` key.
        if options_has_body_key(call, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Route reads `body` but has no `body:` schema in options — Elysia will not validate the payload.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Check if any object argument in the call has a `body:` property key
/// (skipping function/arrow arguments which are the handler).
fn options_has_body_key(
    call: &oxc_ast::ast::CallExpression,
    source: &str,
) -> bool {
    use oxc_ast::ast::Argument;
    for arg in &call.arguments {
        match arg {
            Argument::ObjectExpression(obj) => {
                if obj_has_body_key(&obj.properties, source) {
                    return true;
                }
            }
            _ => continue,
        }
    }
    false
}

fn obj_has_body_key(
    properties: &oxc_allocator::Vec<oxc_ast::ast::ObjectPropertyKind>,
    _source: &str,
) -> bool {
    use oxc_ast::ast::{ObjectPropertyKind, PropertyKey};
    for prop in properties.iter() {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        match &p.key {
            PropertyKey::StaticIdentifier(id) if id.name == "body" => {
                return true;
            }
            PropertyKey::StringLiteral(s) if s.value == "body" => {
                return true;
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_post_with_body_no_schema() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_post_with_body_schema() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ a: t.String() }) });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_post_with_model_ref() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: 'user.create' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.post('/x', ({ body }) => body);";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }

    #[test]
    fn allows_post_with_typed_reference() {
        let src = "import { Elysia } from 'elysia';\nimport { UserSchema } from './schemas';\nnew Elysia().post('/x', ({ body }) => body, { body: UserSchema });";
        assert!(run_on(src).is_empty());
    }
}
