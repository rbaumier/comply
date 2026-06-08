//! elysia-no-mix-zod-typebox backend — flag mixing Zod with Elysia's `t`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement"] prefilter = ["zod"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    // Trigger on the zod import line, but only when the file also uses Elysia's `t`.
    let is_zod = text.contains("from 'zod'") || text.contains("from \"zod\"");
    if !is_zod {
        return;
    }

    let uses_t = ctx.source_contains("t.Object(")
        || ctx.source_contains("t.String(")
        || ctx.source_contains("t.Number(")
        || ctx.source_contains("t.Array(")
        || ctx.source_contains("t.Boolean(");
    if !uses_t {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-no-mix-zod-typebox".into(),
        message: "File uses both Zod and Elysia's `t` validators — pick one. Mixing breaks Elysia's static type inference.".into(),
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
    fn flags_mixed_zod_and_t() {
        let src = "import { Elysia, t } from 'elysia';\nimport { z } from 'zod';\nconst s = t.Object({ a: t.String() });\nconst z2 = z.object({});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_only_t() {
        let src = "import { Elysia, t } from 'elysia';\nconst s = t.Object({ a: t.String() });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_only_zod() {
        let src =
            "import { Elysia } from 'elysia';\nimport { z } from 'zod';\nconst s = z.object({});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "import { z } from 'zod';\nconst x = t.Object({});";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
