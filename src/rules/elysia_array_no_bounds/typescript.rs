//! elysia-array-no-bounds backend — flag `t.Array(...)` calls that lack
//! `minItems` / `maxItems` bounds.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["maxItems", "minItems"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" { return; }
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "t.Array" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");

    if args_text.contains("minItems") || args_text.contains("maxItems") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-array-no-bounds".into(),
        message: "`t.Array(...)` has no `minItems`/`maxItems` — unbounded arrays let clients send huge payloads.".into(),
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
    fn flags_unbounded_array() {
        let src = "import { t } from 'elysia';\nconst s = t.Array(t.String());";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_array_with_max_items() {
        let src = "import { t } from 'elysia';\nconst s = t.Array(t.String(), { maxItems: 100 });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_array_with_min_items_only() {
        let src = "import { t } from 'elysia';\nconst s = t.Array(t.String(), { minItems: 1 });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "const s = t.Array(t.String());";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
