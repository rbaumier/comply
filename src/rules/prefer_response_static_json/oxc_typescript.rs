//! prefer-response-static-json oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectExpression, ObjectPropertyKind};
use std::sync::Arc;

pub struct Check;

/// `true` when the `ResponseInit` literal sets an explicit `Content-Type` other
/// than `application/json`. `Response.json()` always emits `application/json`,
/// so a deliberate media type (e.g. RFC 7807 `application/problem+json`) cannot
/// be expressed by the rewrite this rule suggests — flagging it is a false
/// positive. When `headers` is present but not an inspectable object literal
/// (a `Headers` instance, a variable, a spread…) the value can't be proven to
/// be JSON, so it is conservatively treated as custom.
fn sets_non_json_content_type(init: &ObjectExpression) -> bool {
    for prop in &init.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            return true;
        };
        if !p.key.static_name().is_some_and(|n| n == "headers") {
            continue;
        }
        let Expression::ObjectExpression(headers) = &p.value else {
            return true;
        };
        for h in &headers.properties {
            let ObjectPropertyKind::ObjectProperty(hp) = h else {
                return true;
            };
            let Some(name) = hp.key.static_name() else {
                continue;
            };
            if !name.eq_ignore_ascii_case("content-type") {
                continue;
            }
            let Expression::StringLiteral(lit) = &hp.value else {
                return true;
            };
            let value = lit.value.as_str().trim().to_ascii_lowercase();
            return value != "application/json" && !value.starts_with("application/json;");
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Response"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };

        // Constructor must be `Response`.
        let Expression::Identifier(ident) = &new_expr.callee else {
            return;
        };
        if ident.name.as_str() != "Response" {
            return;
        }

        // Must have at least one argument.
        let Some(first_arg) = new_expr.arguments.first() else {
            return;
        };

        // First argument must be a call expression: JSON.stringify(...).
        let oxc_ast::ast::Argument::CallExpression(call) = first_arg else {
            return;
        };

        // Callee must be `JSON.stringify`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "JSON" || member.property.name.as_str() != "stringify" {
            return;
        }

        // Skip responses that set a deliberate non-JSON Content-Type.
        if let Some(oxc_ast::ast::Argument::ObjectExpression(init)) = new_expr.arguments.get(1) {
            if sets_non_json_content_type(init) {
                return;
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `Response.json(data)` over `new Response(JSON.stringify(data))`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_new_response_json_stringify() {
        let d = run_on(
            r#"return new Response(JSON.stringify(data), { headers: { "Content-Type": "application/json" } });"#,
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-response-static-json");
    }

    #[test]
    fn flags_bare_new_response_json_stringify() {
        assert_eq!(run_on("const res = new Response(JSON.stringify({ ok: true }));").len(), 1);
    }

    #[test]
    fn allows_response_json() {
        assert!(run_on("return Response.json(data);").is_empty());
    }

    #[test]
    fn allows_new_response_with_string() {
        assert!(run_on(r#"return new Response("hello");"#).is_empty());
    }

    // Regression: issue #20 — RFC 7807 Problem responses set
    // `application/problem+json`, which `Response.json()` cannot emit, so a
    // `new Response(JSON.stringify(...))` carrying a deliberate non-JSON
    // Content-Type must not be flagged.
    #[test]
    fn allows_custom_content_type() {
        let src = r#"new Response(JSON.stringify(body), {
            status: 400,
            headers: { "Content-Type": "application/problem+json" },
        });"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_application_json_with_charset() {
        let src = r#"new Response(JSON.stringify(data), {
            headers: { "Content-Type": "application/json; charset=utf-8" },
        });"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
