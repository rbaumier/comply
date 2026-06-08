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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "src/App.test.tsx")
    }

    #[test]
    fn flags_cleanup_import() {
        let d = run_on("import { cleanup } from '@testing-library/react';");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-manual-rtl-cleanup");
    }

    #[test]
    fn flags_cleanup_among_other_imports() {
        let d = run_on("import { render, cleanup } from '@testing-library/react';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_render_only() {
        assert!(run_on("import { render } from '@testing-library/react';").is_empty());
    }
}
