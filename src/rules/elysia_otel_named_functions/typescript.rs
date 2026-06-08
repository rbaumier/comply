//! elysia-otel-named-functions backend — flag arrow functions in `.derive`/`.resolve` under opentelemetry.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    let last = callee_text.rsplit('.').next().unwrap_or("");
    if last != "derive" && last != "resolve" {
        return;
    }
    if !callee_text.contains('.') {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let named: Vec<_> = args.named_children(&mut cursor).collect();
    // The handler is the last argument (first arg may be a scope string/options).
    let Some(handler) = named.last() else { return };
    if handler.kind() != "arrow_function" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-otel-named-functions".into(),
        message: "Arrow function in `.derive`/`.resolve` — OpenTelemetry spans will be unnamed; use a named function.".into(),
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
    fn flags_arrow_in_derive() {
        let src = "import { opentelemetry } from '@elysiajs/opentelemetry';\napp.derive(async ({ headers }) => ({ user: null }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_named_function_in_derive() {
        let src = "import { opentelemetry } from '@elysiajs/opentelemetry';\napp.derive(async function deriveUser({ headers }) { return { user: null }; });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_otel_files() {
        let src = "app.derive(async ({ headers }) => ({ user: null }));";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
