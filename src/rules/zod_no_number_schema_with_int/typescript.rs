//! zod-no-number-schema-with-int backend — flag `z.number().int()`.
//!
//! Zod v4 exposes `z.int()` as a dedicated integer schema. The legacy
//! `z.number().int()` chain creates a number schema and then refines it,
//! which is slower and more verbose than the direct `z.int()` schema.
//!
//! `z.int()` exists only in zod v4, so the suggestion fires only when the
//! nearest `package.json` proves zod resolves to v4+; on zod v3 (or an
//! unresolvable version) `z.int()` does not exist and the nudge is suppressed.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["z.number"] => |node, source, ctx, diagnostics|
    // The outer call must be `<something>.int()`.
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" { return; }
    let Some(prop) = callee.child_by_field_name("property") else { return; };
    let Ok("int") = prop.utf8_text(source) else { return; };

    // The object being `.int()`-ed must itself be `z.number()`.
    let Some(object) = callee.child_by_field_name("object") else { return; };
    if object.kind() != "call_expression" { return; }
    let Some(inner_fn) = object.child_by_field_name("function") else { return; };
    let Ok(inner_text) = inner_fn.utf8_text(source) else { return; };
    if inner_text != "z.number" { return; }

    // `z.int()` exists only in Zod v4. On a project pinned to zod < 4 (or where
    // the version is unresolvable) it does not exist, so the suggested `z.int()`
    // would be a runtime error. Fire only when the nearest package.json proves
    // zod resolves to v4+.
    if !crate::rules::zod_helpers::zod_is_v4_or_later(ctx) { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "zod-no-number-schema-with-int".into(),
        message: "`z.number().int()` can be replaced by `z.int()` in Zod v4+."
            .into(),
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
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;

    /// Run the rule against `source` written to `rel_path` inside a temp project
    /// whose `package.json` is `pkg_json` (or no manifest when `None`), so the
    /// zod-version gate resolves against a real nearest manifest.
    fn run_in_project(pkg_json: Option<&str>, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        if let Some(pkg) = pkg_json {
            fs::write(dir.path().join("package.json"), pkg).unwrap();
        }
        let src_path = dir.path().join(rel_path);
        fs::create_dir_all(src_path.parent().unwrap()).unwrap();
        fs::write(&src_path, source).unwrap();
        let src_path = fs::canonicalize(&src_path).unwrap();

        let source_file = SourceFile {
            path: src_path.clone(),
            language: Language::TypeScript,
        };
        let refs: Vec<&SourceFile> = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let file =
            crate::rules::file_ctx::FileCtx::build(&src_path, source, Language::TypeScript, &project);

        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &src_path, &project, &file)
    }

    const V4_PKG: &str = r#"{"name":"app","version":"1.0.0","dependencies":{"zod":"^4.0.0"}}"#;

    #[test]
    fn flags_z_number_int_on_v4() {
        assert_eq!(
            run_in_project(Some(V4_PKG), "app.ts", "const s = z.number().int();").len(),
            1
        );
    }

    #[test]
    fn allows_z_int_on_v4() {
        assert!(run_in_project(Some(V4_PKG), "app.ts", "const s = z.int();").is_empty());
    }

    #[test]
    fn allows_z_number_positive_on_v4() {
        assert!(
            run_in_project(Some(V4_PKG), "app.ts", "const s = z.number().positive();").is_empty()
        );
    }

    // Issue #7388: on a project pinned to zod v3 the suggested `z.int()` does not
    // exist, so `z.number().int()` must not be flagged.
    #[test]
    fn silent_on_v3_project_issue7388() {
        let pkg = r#"{"name":"core","version":"1.0.0","dependencies":{"zod":"3.25.76"}}"#;
        let src = r#"import { z } from "zod"; export const S = z.object({ duration: z.number().int().positive().optional() });"#;
        assert!(run_in_project(Some(pkg), "packages/core/src/schemas/errors.ts", src).is_empty());
    }

    #[test]
    fn silent_on_v3_or_v4_range() {
        let pkg = r#"{"name":"app","version":"1.0.0","dependencies":{"zod":"^3 || ^4"}}"#;
        assert!(run_in_project(Some(pkg), "app.ts", "const s = z.number().int();").is_empty());
    }

    #[test]
    fn silent_without_package_json() {
        assert!(run_in_project(None, "app.ts", "const s = z.number().int();").is_empty());
    }
}
