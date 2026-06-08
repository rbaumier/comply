//! elysia-file-magic-number backend — flag `z.file()` without a magic-number check.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia")
        || !ctx.source_contains("zod")
        || !ctx.source_contains("elysia")
    {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.utf8_text(source).unwrap_or("") != "z.file" {
        return;
    }

    if ctx.source_contains("fileType(") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-file-magic-number".into(),
        message: "`z.file()` only checks the MIME header — verify the magic number with `fileType()`.".into(),
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
    fn flags_bare_z_file() {
        let src = "import { Elysia } from 'elysia';\nimport { z } from 'zod';\napp.post('/upload', h, { body: z.object({ file: z.file() }) });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_z_file_with_options() {
        let src = "import { Elysia } from 'elysia';\nimport { z } from 'zod';\nconst s = z.file({ type: 'image/png' });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_z_file_with_filetype_refine() {
        let src = "import { Elysia } from 'elysia';\nimport { z } from 'zod';\nimport { fileType } from 'file-type';\nconst s = z.file().refine(b => fileType(b)?.mime === 'image/png');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "import { z } from 'zod';\nconst s = z.file();";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn no_fp_on_zod_file_schema_constructor_without_elysia_import() {
        // z.file() is Zod's file schema constructor; it should not fire in
        // a file that imports only zod, even if the project uses Elysia.
        let src = "import { z } from 'zod';\nconst FileSchema = z.file();";
        assert!(run_on(src).is_empty());
    }
}
