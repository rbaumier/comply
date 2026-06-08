//! elysia-model-export-types backend — when a file exports a `t.Object(...)`
//! const, expect a corresponding `typeof X.static` type alias.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let _ = source;
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let norm: String = ctx.source.chars().filter(|c| !c.is_whitespace()).collect();

    let exports_typebox_const = norm.contains("exportconst") && norm.contains("=t.Object(");
    if !exports_typebox_const { return; }

    if norm.contains(".static") { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-model-export-types".into(),
        message: "Module exports a `t.Object(...)` schema but no `typeof X.static` type — consumers cannot annotate variables with the model type.".into(),
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
    fn flags_schema_without_static_type() {
        let src = "import { t } from 'elysia';\nexport const User = t.Object({ id: t.Number() });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_schema_with_static_type() {
        let src = "import { t } from 'elysia';\nexport const User = t.Object({ id: t.Number() });\nexport type User = typeof User.static;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_file_with_no_typebox_export() {
        let src = "import { Elysia } from 'elysia';\nexport const app = new Elysia();";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "export const User = t.Object({ id: t.Number() });";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
