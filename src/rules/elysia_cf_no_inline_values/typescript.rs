//! elysia-cf-no-inline-values backend — flag string-literal route handlers under `CloudflareAdapter`.

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "options", "head", "all",
];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    let Some(method) = callee_text.rsplit('.').next() else { return };
    if !ROUTE_METHODS.contains(&method) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let named: Vec<_> = args.named_children(&mut cursor).collect();
    if named.len() < 2 {
        return;
    }
    let second = named[1];
    if second.kind() != "string" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-cf-no-inline-values".into(),
        message: "Inline string handler under `CloudflareAdapter` — wrap the value in an arrow function.".into(),
        severity: Severity::Error,
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
    fn flags_inline_string_handler() {
        let src = "import { CloudflareAdapter } from 'elysia/adapter/cloudflare';\napp.get('/', 'Hello');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_function_handler() {
        let src = "import { CloudflareAdapter } from 'elysia/adapter/cloudflare';\napp.get('/', () => 'Hello');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_cf_files() {
        let src = "app.get('/', 'Hello');";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
