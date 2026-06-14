use std::sync::Arc;

use oxc_ast::ast::{Expression, StaticMemberExpression};
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

/// True when `member` is the left-hand side (assignment target) of an
/// `AssignmentExpression`, i.e. a CommonJS `exports.x = …` write rather than a
/// read of an `exports.x` property.
fn is_assignment_target(
    node: &oxc_semantic::AstNode,
    member: &StaticMemberExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    let AstKind::AssignmentExpression(assign) = parent.kind() else {
        return false;
    };
    assign.left.span() == member.span
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::CallExpression,
            AstType::IdentifierReference,
            AstType::StaticMemberExpression,
        ]
    }

    // The rule only fires on `require(…)`, `__dirname`, `__filename`,
    // `module.exports`, or `exports.x`. `"exports"` is a substring of both
    // `module.exports` and `exports.x`, so these four literals cover every
    // path. Pure-ESM files (the common case) carry none and skip dispatch.
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["require", "__dirname", "__filename", "exports"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !crate::rules::module_system::is_es_module_context_cached(ctx) {
            return;
        }

        match node.kind() {
            // `require("…")` calls
            AstKind::CallExpression(call) => {
                let Expression::Identifier(ident) = &call.callee else {
                    return;
                };
                if ident.name.as_str() != "require" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Use `import` instead of `require()` — prefer ESM over CommonJS."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            // `__dirname` / `__filename` identifiers
            AstKind::IdentifierReference(ident) => {
                let msg = match ident.name.as_str() {
                    "__dirname" => {
                        "Use `import.meta.dirname` instead of `__dirname`."
                    }
                    "__filename" => {
                        "Use `import.meta.filename` instead of `__filename`."
                    }
                    _ => return,
                };
                // Only the implicit CommonJS global is wrong here. A file that
                // declares its own `const __dirname = …` (the `import.meta.url`
                // polyfill) is using a legitimate local binding.
                if !semantic.is_reference_to_global_variable(ident) {
                    return;
                }
                // Vitest injects `__dirname`/`__filename` as globals into ESM
                // test files for backwards compatibility, so a test file in a
                // Vitest project legitimately relies on the shim — rewriting to
                // `import.meta.dirname` would drop that reliance. Skip the check
                // only when both signals hold: the file is a test file and the
                // project uses Vitest.
                if ctx.file.path_segments.in_test_dir && ctx.project.uses_vitest(ctx.path) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, ident.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: msg.into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            // `module.exports` or `exports.foo`
            AstKind::StaticMemberExpression(member) => {
                let Expression::Identifier(obj) = &member.object else {
                    return;
                };
                let obj_name = obj.name.as_str();
                let prop_name = member.property.name.as_str();

                if obj_name == "module" && prop_name == "exports" {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, member.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message:
                            "Use `export` instead of `module.exports` — prefer ESM over CommonJS."
                                .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                } else if obj_name == "exports" && is_assignment_target(node, member, semantic) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, member.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message:
                            "Use `export` instead of `exports.x = …` — prefer ESM over CommonJS."
                                .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "module.mjs")
    }

    #[test]
    fn flags_exports_assignment() {
        let d = run_on("exports.foo = 1;");
        assert!(d.iter().any(|d| d.message.contains("exports.x")));
    }

    #[test]
    fn allows_exports_member_read() {
        let d = run_on(
            r#"
            const exports = (await import('axios'));
            exports.isCancel;
            (exports.axios ?? exports.default);
            "#,
        );
        assert!(d.is_empty(), "reads of `exports.x` must not be flagged: {d:?}");
    }

    #[test]
    fn flags_module_exports() {
        let d = run_on("module.exports = foo;");
        assert!(d.iter().any(|d| d.message.contains("module.exports")));
    }

    #[test]
    fn flags_cjs_global_dirname() {
        let d = run_on("const BENCH_DIR = path.resolve(__dirname, '..');");
        assert!(
            d.iter().any(|d| d.message.contains("import.meta.dirname")),
            "the implicit CJS global `__dirname` must be flagged: {d:?}"
        );
    }

    #[test]
    fn allows_user_defined_dirname() {
        let d = run_on(
            r#"
            import path from 'node:path'
            import url from 'node:url'

            const __dirname = path.dirname(url.fileURLToPath(import.meta.url))
            const BENCH_DIR = path.resolve(__dirname, '..')
            "#,
        );
        assert!(
            d.is_empty(),
            "a user-defined `const __dirname` binding must not be flagged: {d:?}"
        );
    }

    /// Run the rule against `source` written to `rel_path` inside a temp project
    /// whose `package.json` is `pkg_json` (or no manifest when `None`). Builds a
    /// real `FileCtx` so `in_test_dir` reflects the path.
    fn run_in_project(pkg_json: Option<&str>, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        use std::fs;
        use tempfile::TempDir;

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
        let file = crate::rules::file_ctx::FileCtx::build(
            &src_path,
            source,
            Language::TypeScript,
            &project,
        );

        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &src_path, &project, &file)
    }

    const VITEST_PKG: &str =
        r#"{"name":"app","version":"1.0.0","type":"module","devDependencies":{"vitest":"^2.0.0"}}"#;

    #[test]
    fn allows_dirname_in_vitest_test_file() {
        let d = run_in_project(
            Some(VITEST_PKG),
            "test/fixtures.test.ts",
            r#"
            import { resolve } from 'node:path'
            const root = resolve(__dirname, 'fixtures/vite')
            "#,
        );
        assert!(
            d.is_empty(),
            "implicit `__dirname` in a Vitest test file is the injected shim, not CJS: {d:?}"
        );
    }

    #[test]
    fn flags_dirname_in_non_test_file_of_vitest_project() {
        let d = run_in_project(
            Some(VITEST_PKG),
            "src/paths.ts",
            "const BENCH_DIR = path.resolve(__dirname, '..');",
        );
        assert!(
            d.iter().any(|d| d.message.contains("import.meta.dirname")),
            "`__dirname` outside a test file must still be flagged even in a Vitest project: {d:?}"
        );
    }

    #[test]
    fn flags_dirname_in_test_file_without_vitest() {
        let pkg = r#"{"name":"app","version":"1.0.0","type":"module"}"#;
        let d = run_in_project(
            Some(pkg),
            "test/fixtures.test.ts",
            "const root = path.resolve(__dirname, 'fixtures/vite');",
        );
        assert!(
            d.iter().any(|d| d.message.contains("import.meta.dirname")),
            "`__dirname` in a test file without Vitest has no shim and must still be flagged: {d:?}"
        );
    }
}
