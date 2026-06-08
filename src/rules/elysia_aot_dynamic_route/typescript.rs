//! elysia-aot-dynamic-route — flag `.get(<dynamic>, ...)`/`.post(...)`/etc.
//! when the first argument is a template_string with substitutions or a
//! binary_expression (string concatenation) instead of a plain string.
//!
//! `.get` / `.post` / etc. are widely overloaded names (HTTP clients, test
//! helpers, Map.get). The rule only fires when:
//!   - the file imports `elysia` or `@elysiajs/...`
//!   - the file is not a test file (`.test.ts`, `.spec.ts`, `__tests__/`)
//!   - the call isn't a `fetch(`...`)` (already excluded — fetch's callee
//!     is an identifier, not a `member_expression`).

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options", "route",
];

fn is_dynamic_path(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    match node.kind() {
        "template_string" => {
            // Plain backtick string with no `${...}` is fine.
            let text = node.utf8_text(source).unwrap_or("");
            text.contains("${")
        }
        "binary_expression" => {
            // `'/users/' + id` — concatenation.
            let text = node.utf8_text(source).unwrap_or("");
            text.contains('+')
        }
        _ => false,
    }
}

fn imports_elysia(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "from 'elysia'")
        || crate::oxc_helpers::source_contains(source, "from \"elysia\"")
        || crate::oxc_helpers::source_contains(source, "from 'elysia/")
        || crate::oxc_helpers::source_contains(source, "from \"elysia/")
        || crate::oxc_helpers::source_contains(source, "from '@elysiajs/")
        || crate::oxc_helpers::source_contains(source, "from \"@elysiajs/")
}

fn is_test_file(path: &std::path::Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if name.contains(".test.") || name.contains(".spec.") {
        return true;
    }
    path.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("__tests__") | Some("__test__") | Some("tests") | Some("test")
        )
    })
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !imports_elysia(ctx.source) {
        return;
    }
    if is_test_file(ctx.path) {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let name = prop.utf8_text(source).unwrap_or("");
    if !ROUTE_METHODS.contains(&name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(first) = args.named_children(&mut cursor).next() else { return };
    if !is_dynamic_path(first, source) {
        return;
    }
    let pos = first.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-aot-dynamic-route".into(),
        message: "Route path built dynamically (template literal / concatenation) — Elysia AOT can only compile static path strings. Use `:param` segments instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_template_literal_with_substitution() {
        let src = "import { Elysia } from 'elysia';\napp.get(`/users/${id}`, () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_string_concatenation() {
        let src = "import { Elysia } from 'elysia';\napp.post('/users/' + id, () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_static_string() {
        let src = "import { Elysia } from 'elysia';\napp.get('/users/:id', () => 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_plain_template_string() {
        let src = "import { Elysia } from 'elysia';\napp.get(`/users/:id`, () => 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get(`/users/${id}`, () => 'ok');";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn ignores_fetch_with_template_literal() {
        // Regression: `fetch(`...`)` is a global call, not a route definition.
        // Its callee is an identifier, not a member_expression — already
        // filtered, but we keep this test to lock the behaviour.
        let src = "import { Elysia } from 'elysia';\nconst body = await fetch(`/users/${id}`);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_test_helper_in_test_file() {
        // Regression: test helpers like `client.get(`/users/${id}`)` use the
        // same method names as Elysia routes. Skip files under `__tests__/`
        // or named `*.test.ts` / `*.spec.ts`.
        use crate::project::ProjectCtx;
                use std::path::Path;
        let project = ProjectCtx::for_test_with_framework("elysia");
        let src = "import { Elysia } from 'elysia';\nconst client = makeClient();\nawait client.get(`/users/${id}`);";
        assert!(
            crate::rules::test_helpers::run_rule_with_ctx(&Check, src, Path::new("src/users.test.ts"), &project, crate::rules::file_ctx::default_static_file_ctx())
                .is_empty()
        );
        assert!(
            crate::rules::test_helpers::run_rule_with_ctx(&Check, src, Path::new("__tests__/users.ts"), &project, crate::rules::file_ctx::default_static_file_ctx())
                .is_empty()
        );
    }

    #[test]
    fn ignores_file_without_elysia_import() {
        // Regression: a fetch helper in a non-Elysia file with `someClient.get(`/x/${id}`)`.
        let src = "const client = makeClient();\nawait client.get(`/users/${id}`);";
        assert!(run_on(src).is_empty());
    }
}
