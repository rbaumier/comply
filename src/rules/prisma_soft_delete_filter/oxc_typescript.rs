//! prisma-soft-delete-filter oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const FIND_METHODS: &[&str] = &["findMany", "findFirst", "findUnique"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".findMany(", ".findFirst(", ".findUnique("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !FIND_METHODS.contains(&method) {
            return;
        }
        // Only fire when the file mentions `prisma` somewhere — keeps
        // the rule from misfiring on Drizzle / unrelated APIs that may
        // happen to expose the same method name.
        if !ctx.source_contains("prisma") {
            return;
        }
        // When a schema.prisma is available, skip models that don't have a
        // `deletedAt` field — they can't have soft-deleted rows, so the
        // missing filter is not a bug. Fall through when no schema is found
        // (backward-compatible: fire on all).
        if let Expression::StaticMemberExpression(inner) = &member.object {
            let model_name = inner.property.name.as_str();
            if ctx.project.prisma_model_is_soft_delete(ctx.path, model_name) == Some(false) {
                // A schema governs this file's package and this model has no
                // `deletedAt` column — there are no soft-deleted rows, so the
                // missing filter is not a bug. (`None` = no schema → fall
                // through to fire-on-all; `Some(true)` = the model is
                // soft-delete → fall through to flag.)
                return;
            }
        }
        // Heuristic: scan the entire call source range for
        // `deletedAt` — present anywhere in the where clause is fine.
        let span_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        if span_text.contains("deletedAt") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{method}` without a `deletedAt` filter — soft-deleted rows will \
                 leak into the result. Add `where: {{ deletedAt: null, … }}`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    fn run_gated(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, src, path)
    }

    fn run_with_project(src: &str, path: &std::path::Path, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, path, project, crate::rules::file_ctx::default_static_file_ctx())
    }

    /// Build a single-package project under a tempdir whose `prisma/schema.prisma`
    /// declares an `Envelope` model with `deletedAt` (and an `Account` without),
    /// returning the loaded `ProjectCtx` and a source file path inside the package.
    fn project_with_envelope_schema() -> (tempfile::TempDir, ProjectCtx, std::path::PathBuf) {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name":"app"}"#).unwrap();
        std::fs::create_dir_all(dir.path().join("prisma")).unwrap();
        std::fs::write(
            dir.path().join("prisma/schema.prisma"),
            "model Account {\n  id String @id\n}\n\nmodel Envelope {\n  id String @id\n  deletedAt DateTime?\n}\n",
        )
        .unwrap();
        let file_path = dir.path().join("src/repo.ts");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "export const x = 1;").unwrap();
        let source = SourceFile { path: file_path.clone(), language: Language::TypeScript };
        let ctx = ProjectCtx::load(&[&source], &Config::default());
        (dir, ctx, file_path)
    }

    #[test]
    fn flags_find_many_without_deleted_at() {
        let src = r#"const r = await prisma.user.findMany({ where: { active: true } });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_find_many_with_deleted_at() {
        let src = r#"const r = await prisma.user.findMany({ where: { deletedAt: null, active: true } });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_prisma_callers() {
        let src = r#"const r = obj.findMany({ where: { active: true } });"#;
        assert!(run(src).is_empty());
    }

    // Regression tests for issue #836: FP on models without deletedAt field.

    #[test]
    fn ignores_find_first_on_model_without_deleted_at_in_schema() {
        // schema contains "envelope" with deletedAt, but not "user"
        let (_dir, project, file_path) = project_with_envelope_schema();
        let src =
            r#"const u = await prisma.user.findFirst({ where: { email: "x" } });"#;
        assert!(run_with_project(src, &file_path, &project).is_empty());
    }

    #[test]
    fn flags_find_first_on_model_with_deleted_at_in_schema() {
        let (_dir, project, file_path) = project_with_envelope_schema();
        let src =
            r#"const e = await prisma.envelope.findFirst({ where: { id: "1" } });"#;
        assert_eq!(run_with_project(src, &file_path, &project).len(), 1);
    }

    // Regression for issue #1358: schemas defined via TypeScript template strings
    // (no static `.prisma` file) leave the rule with no model list, so the
    // schema-less fallback fired on every query in the test suite. Soft-delete
    // enforcement is a production data-integrity concern, so the rule is gated
    // out of test directories.

    #[test]
    fn ignores_find_many_in_test_dir_without_schema() {
        let src = r#"const r = await prisma.user.findMany({ where: { active: true } });"#;
        assert!(
            run_gated(src, "packages/client/tests/functional/tests_m-to-n.ts").is_empty()
        );
    }

    #[test]
    fn flags_find_many_in_production_dir_without_schema() {
        let src = r#"const r = await prisma.user.findMany({ where: { active: true } });"#;
        assert_eq!(run_gated(src, "src/repositories/user.ts").len(), 1);
    }
}
