//! hono-csrf-missing backend — flag mutation routes without CSRF protection.

use crate::diagnostic::{Diagnostic, Severity};

const MUTATION_METHODS: &[&str] = &["post", "put", "delete", "patch"];

crate::ast_check! { on ["call_expression"] prefilter = ["hono"] => |node, source, ctx, diagnostics|
    // Only check Hono files.
    if !ctx.source_contains("from 'hono'") && !ctx.source_contains("from \"hono\"") {
        return;
    }

    // Skip if CSRF protection is already imported.
    if ctx.source_contains("hono/csrf") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(method) = callee.child_by_field_name("property") else { return };
    let method_name = method.utf8_text(source).unwrap_or("");

    if !MUTATION_METHODS.contains(&method_name) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "hono-csrf-missing".into(),
        message: "Mutation route without CSRF protection — add `app.use(csrf())`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_post_without_csrf() {
        let src =
            "import { Hono } from 'hono';\nconst app = new Hono();\napp.post('/api', handler);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_multiple_mutation_routes() {
        let src = "import { Hono } from 'hono';\napp.post('/a', h);\napp.put('/b', h);\napp.delete('/c', h);";
        assert_eq!(run_on(src).len(), 3);
    }

    #[test]
    fn allows_with_csrf_import() {
        let src = "import { Hono } from 'hono';\nimport { csrf } from 'hono/csrf';\napp.use(csrf());\napp.post('/api', handler);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "app.post('/api', handler);";
        assert!(run_on(src).is_empty());
    }
}
