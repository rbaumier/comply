//! elysia-cors-methods-wildcard backend — flag credentialed `cors()` without an explicit `methods` list.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.utf8_text(source).unwrap_or("") != "cors" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

    if !norm.contains("credentials:true") {
        return;
    }
    if norm.contains("methods:") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-cors-methods-wildcard".into(),
        message: "`credentials: true` without an explicit `methods` list — every HTTP verb is allowed.".into(),
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
    fn flags_credentials_without_methods() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors({ origin: 'https://x.com', credentials: true }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_credentials_with_methods() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors({ origin: 'https://x.com', credentials: true, methods: ['GET', 'POST'] }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_no_credentials() {
        let src =
            "import { cors } from '@elysiajs/cors';\napp.use(cors({ origin: 'https://x.com' }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_cors() {
        let src = "app.use(cors({ credentials: true }));";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
