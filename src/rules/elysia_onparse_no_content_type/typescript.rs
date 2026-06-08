//! elysia-onparse-no-content-type backend — flag onParse handlers ignoring contentType.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["\"onParse\""] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "onParse" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    if args_text.contains("contentType") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-onparse-no-content-type".into(),
        message: "`onParse` handler should inspect `contentType` and only handle formats it understands; otherwise it can break default parsing.".into(),
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
    fn flags_onparse_without_content_type() {
        let src = "import { Elysia } from 'elysia';\napp.onParse(({ request }) => request.text());";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_onparse_with_unrelated_logic() {
        let src = "import { Elysia } from 'elysia';\napp.onParse(async ({ request }) => {\n  const body = await request.text();\n  return JSON.parse(body);\n});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_onparse_with_content_type_check() {
        let src = "import { Elysia } from 'elysia';\napp.onParse(({ request, contentType }) => {\n  if (contentType === 'application/x-yaml') return parseYaml(request);\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.onParse(() => null);";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
