//! elysia-openapi-security-scheme backend — flag route-level `security:` without a `securitySchemes` definition.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if ctx.source.contains("securitySchemes") {
        return;
    }

    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key.utf8_text(source).unwrap_or("");
    let key_norm = key_text.trim_matches(|c: char| c == '\'' || c == '"');
    if key_norm != "security" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-openapi-security-scheme".into(),
        message: "Route declares `security:` but no `securitySchemes` is defined — the OpenAPI document will be invalid.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
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
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
