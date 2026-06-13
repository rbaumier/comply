//! OxcCheck backend for no-manual-rtl-cleanup.
//!
//! Detects manual `cleanup` imports from `@testing-library` in test files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@testing-library"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }

        // Auto-cleanup is Vitest-specific: under Jest, `afterEach(cleanup)` is
        // the documented, required pattern, so removing it pollutes tests.
        if !ctx.project.uses_vitest(ctx.path) {
            return;
        }

        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };

        if !import.source.value.as_str().contains("@testing-library") {
            return;
        }

        let Some(specifiers) = &import.specifiers else {
            return;
        };
        let has_cleanup = specifiers.iter().any(|spec| {
            if let oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(named) = spec {
                return named.imported.name().as_str() == "cleanup";
            }
            false
        });
        if !has_cleanup {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Manual `cleanup` import from `@testing-library` — \
                      Vitest runs cleanup automatically after each test."
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
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use tempfile::TempDir;

    /// Run the rule against a `.test.tsx` file in a temp project carrying the
    /// given `package.json`, so the Vitest gate sees a real manifest on disk.
    fn run_with_pkg(pkg_json: &str, src: &str) -> Vec<Diagnostic> {
        run_in_project(|dir| {
            fs::write(dir.join("package.json"), pkg_json).unwrap();
        }, src)
    }

    /// Same, but the caller seeds the project layout (manifest, config files).
    fn run_in_project(seed: impl FnOnce(&std::path::Path), src: &str) -> Vec<Diagnostic> {
        crate::oxc_helpers::reset_file_caches();
        let dir = TempDir::new().unwrap();
        seed(dir.path());
        let path = dir.path().join("App.test.tsx");
        fs::write(&path, src).unwrap();

        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, src, SourceType::tsx()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test(&path, src);
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if Check.interested_kinds().contains(&node.kind().ty()) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    const VITEST_PKG: &str = r#"{"name":"app","devDependencies":{"vitest":"^1"}}"#;

    #[test]
    fn flags_cleanup_import_in_vitest_project() {
        let d = run_with_pkg(VITEST_PKG, "import { cleanup } from '@testing-library/react';");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-manual-rtl-cleanup");
    }

    #[test]
    fn flags_cleanup_among_other_imports_in_vitest_project() {
        let d = run_with_pkg(
            VITEST_PKG,
            "import { render, cleanup } from '@testing-library/react';",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_cleanup_when_only_vitest_config_present() {
        let d = run_in_project(
            |dir| {
                fs::write(dir.join("package.json"), r#"{"name":"app"}"#).unwrap();
                fs::write(dir.join("vitest.config.ts"), "export default {}").unwrap();
            },
            "import { cleanup } from '@testing-library/react';",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_render_only_in_vitest_project() {
        assert!(
            run_with_pkg(VITEST_PKG, "import { render } from '@testing-library/react';").is_empty()
        );
    }

    // Issue #1900: in a Jest project (tsdx/jest, no Vitest anywhere) the manual
    // `afterEach(cleanup)` is the documented, required pattern — the rule must
    // stay silent. Mirrors packages/formik/test/Field.test.tsx.
    #[test]
    fn ignores_cleanup_import_in_jest_project() {
        let pkg = r#"{"name":"formik","scripts":{"test":"tsdx test"},"devDependencies":{"tsdx":"^0.14","@testing-library/react":"^11"}}"#;
        let src = r#"
            import { act, cleanup, render } from '@testing-library/react';
            afterEach(cleanup);
        "#;
        assert!(run_with_pkg(pkg, src).is_empty());
    }

    // No test-runner evidence at all (no vitest dep, no script, no config):
    // the rule defaults to silent rather than guessing.
    #[test]
    fn ignores_cleanup_import_when_runner_unknown() {
        let pkg = r#"{"name":"app"}"#;
        assert!(
            run_with_pkg(pkg, "import { cleanup } from '@testing-library/react';").is_empty()
        );
    }
}
