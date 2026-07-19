//! zod-no-string-schema-with-uuid oxc backend — flag `z.string().uuid()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.string"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression with property `uuid`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "uuid" {
            return;
        }

        // Object must be a call expression (the `z.string()` part).
        let Expression::CallExpression(inner_call) = &member.object else {
            return;
        };

        // Inner callee must be `z.string`.
        let Expression::StaticMemberExpression(inner_member) = &inner_call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &inner_member.object else {
            return;
        };
        if obj.name.as_str() != "z" || inner_member.property.name.as_str() != "string" {
            return;
        }

        // Top-level `z.uuid()` exists only in Zod v4. On a project pinned to zod
        // < 4 (or where the version is unresolvable) it does not exist, so the
        // suggested `z.uuid()` would be a runtime error and `z.string().uuid()`
        // is the correct API. Fire only when the nearest package.json proves zod
        // resolves to v4+.
        if !crate::rules::zod_helpers::zod_is_v4_or_later(ctx) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `z.uuid()` instead of `z.string().uuid()` — the \
                      chained form is deprecated in Zod v4."
                .into(),
            severity: Severity::Error,
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
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::test_helpers::run_rule_with_ctx;
    use std::fs;
    use tempfile::TempDir;

    /// Run the rule against `source` written to `rel_path` inside a temp project
    /// whose `package.json` is `pkg_json` (or no manifest when `None`), so the
    /// zod-version gate resolves against a real nearest manifest. The production
    /// applicability gate (`skip_in_test_dir`) is applied too, so a test-dir path
    /// is suppressed independently of the version gate.
    fn run_in_project(pkg_json: Option<&str>, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        if let Some(pkg) = pkg_json {
            fs::write(dir.path().join("package.json"), pkg).unwrap();
        }
        let src_path = dir.path().join(rel_path);
        fs::create_dir_all(src_path.parent().unwrap()).unwrap();
        fs::write(&src_path, source).unwrap();
        let src_path = fs::canonicalize(&src_path).unwrap();

        let source_file = SourceFile { path: src_path.clone(), language: Language::TypeScript };
        let refs: Vec<&SourceFile> = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let file = crate::rules::file_ctx::FileCtx::build(
            &src_path,
            source,
            Language::TypeScript,
            &project,
        );
        if !super::super::META.applies_to_file(&file) {
            return vec![];
        }

        run_rule_with_ctx(&Check, source, &src_path, &project, &file)
    }

    const V4_PKG: &str = r#"{"name":"app","version":"1.0.0","dependencies":{"zod":"^4.0.0"}}"#;

    #[test]
    fn flags_string_uuid_chain_on_v4() {
        let d = run_in_project(Some(V4_PKG), "app.ts", "const u = z.string().uuid();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("z.uuid()"));
    }

    #[test]
    fn allows_top_level_uuid_on_v4() {
        assert!(run_in_project(Some(V4_PKG), "app.ts", "const u = z.uuid();").is_empty());
    }

    // Issue #4450: zod's own v3 tests (where `z.string().uuid()` is the correct
    // v3 API) and v4/classic backward-compat tests (which deliberately exercise
    // the deprecated chained form) must not be nudged. The central
    // `skip_in_test_dir` gate exempts `/tests/` dirs and `.test.`/`.spec.`
    // infixes. Asserted on a v4 manifest so the version gate would otherwise
    // fire — proving `skip_in_test_dir` is the suppressor.
    #[test]
    fn silent_in_test_dir_issue4450() {
        assert!(
            run_in_project(
                Some(V4_PKG),
                "packages/zod/src/v3/tests/string.test.ts",
                r#"const u = z.string().uuid("custom error");"#,
            )
            .is_empty()
        );
        assert!(
            run_in_project(Some(V4_PKG), "src/schema.test.ts", "const u = z.string().uuid();")
                .is_empty()
        );
    }

    // Suppression is test-scoped: on a v4 project the same chained form in
    // production source is still flagged, preserving the rule's purpose.
    #[test]
    fn flags_string_uuid_chain_in_production_on_v4() {
        assert_eq!(
            run_in_project(Some(V4_PKG), "src/schema.ts", "const u = z.string().uuid();").len(),
            1
        );
    }

    // Issue #7770: Infisical/infisical backend pins zod ^3.22.4, where top-level
    // `z.uuid()` does not exist and `z.string().uuid()` is the correct API, so
    // the chained form must not be flagged on a v3 project.
    #[test]
    fn silent_on_v3_project_issue7770() {
        let pkg = r#"{"name":"backend","version":"1.0.0","dependencies":{"zod":"^3.22.4"}}"#;
        let src = r#"import { z } from "zod"; const p = z.object({ emailDomainId: z.string().uuid().describe("The ID of the email domain to verify") });"#;
        assert!(run_in_project(Some(pkg), "src/ee/routes/v1/email-domain-router.ts", src).is_empty());
    }

    // A range that can resolve to zod v3 (`^3 || ^4`) stays silent: the smallest
    // major it admits is 3, where `z.uuid()` is unavailable.
    #[test]
    fn silent_on_v3_or_v4_range() {
        let pkg = r#"{"name":"app","version":"1.0.0","dependencies":{"zod":"^3 || ^4"}}"#;
        assert!(run_in_project(Some(pkg), "app.ts", "const u = z.string().uuid();").is_empty());
    }

    // Conservative default: with no resolvable zod dependency the version is
    // unproven, so the v4-only suggestion is suppressed.
    #[test]
    fn silent_without_package_json() {
        assert!(run_in_project(None, "app.ts", "const u = z.string().uuid();").is_empty());
    }
}
