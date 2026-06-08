//! OxcCheck backend for elysia-openapi-security-scheme.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }
        if ctx.source_contains("securitySchemes") {
            return;
        }

        let key_name = match &prop.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if key_name != "security" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Route declares `security:` but no `securitySchemes` is defined — the OpenAPI document will be invalid.".into(),
            severity: Severity::Warning,
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
    fn flags_security_without_schemes() {
        let src = "import { openapi } from '@elysiajs/openapi';\napp.get('/me', () => null, { detail: { security: [{ bearerAuth: [] }] } });";
        assert!(!run_on(src).is_empty());
    }


    #[test]
    fn allows_security_with_schemes() {
        let src = "import { openapi } from '@elysiajs/openapi';\napp.use(openapi({ documentation: { components: { securitySchemes: { bearerAuth: { type: 'http', scheme: 'bearer' } } } } }));\napp.get('/me', () => null, { detail: { security: [{ bearerAuth: [] }] } });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_openapi_files() {
        let src = "app.get('/me', () => null, { detail: { security: [{ bearerAuth: [] }] } });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
