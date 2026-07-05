//! OxcCheck backend for zod-record-two-args.

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
        Some(&["record"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "record" {
            return;
        }
        let Expression::Identifier(obj_id) = &member.object else { return };
        if obj_id.name.as_str() != "z" {
            return;
        }

        if call.arguments.len() != 1 {
            return;
        }

        // A `z` resolving to an explicit `zod/v3` / `zod3` subpath import uses
        // the v3 API, where the single-arg `z.record(valueSchema)` form is
        // valid, so skip regardless of the installed version.
        if crate::oxc_helpers::resolves_to_import_from(obj_id, semantic, &["zod/v3", "zod3"]) {
            return;
        }

        // The single-arg form is only removed in Zod v4. Fire only when the
        // nearest `package.json` proves zod resolves to v4+; on zod v3 (or an
        // unresolvable version) the single-arg `z.record(valueSchema)` is valid.
        if !crate::rules::zod_helpers::zod_is_v4_or_later(ctx) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`z.record(valueSchema)` with a single argument is removed in Zod v4 — \
                      pass the key schema explicitly: `z.record(z.string(), valueSchema)`."
                .into(),
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
    fn flags_single_arg_record_on_v4() {
        assert_eq!(
            run_in_project(Some(V4_PKG), "app.ts", "const r = z.record(z.boolean());").len(),
            1
        );
    }

    #[test]
    fn flags_single_arg_record_with_bare_zod_import_on_v4() {
        let src = r#"
            import * as z from "zod";
            const r = z.record(z.boolean());
        "#;
        assert_eq!(run_in_project(Some(V4_PKG), "app.ts", src).len(), 1);
    }

    #[test]
    fn does_not_flag_two_arg_record_on_v4() {
        assert!(
            run_in_project(Some(V4_PKG), "app.ts", "const r = z.record(z.string(), z.boolean());")
                .is_empty()
        );
    }

    // Issue #7387: the project resolves zod to v3 (3.25.76); the single-arg
    // `z.record(valueSchema)` form is valid there, so it must not be flagged.
    #[test]
    fn silent_on_v3_project_issue7387() {
        let pkg = r#"{"name":"core","version":"1.0.0","dependencies":{"zod":"3.25.76"}}"#;
        let src = r#"import { z } from "zod";
            const DeserializedJsonSchema = z.union([LiteralSchema, z.array(DeserializedJsonSchema), z.record(DeserializedJsonSchema)]);"#;
        assert!(run_in_project(Some(pkg), "packages/core/src/schemas/json.ts", src).is_empty());
    }

    #[test]
    fn silent_without_package_json() {
        assert!(run_in_project(None, "app.ts", "const r = z.record(z.boolean());").is_empty());
    }

    // https://github.com/rbaumier/comply/issues/4436 — the `zod/v3` / `zod3`
    // subpath import keeps the single-arg API even under a v4 manifest, so it
    // stays exempt regardless of the resolved version.
    #[test]
    fn does_not_flag_single_arg_record_with_zod_v3_import_on_v4() {
        let src = r#"
            import * as z from "zod/v3";
            const booleanRecord = z.record(z.boolean());
        "#;
        assert!(run_in_project(Some(V4_PKG), "app.ts", src).is_empty());
    }

    #[test]
    fn does_not_flag_single_arg_record_with_zod3_import_on_v4() {
        let src = r#"
            import * as z from "zod3";
            const r = z.record(z.boolean());
        "#;
        assert!(run_in_project(Some(V4_PKG), "app.ts", src).is_empty());
    }
}
