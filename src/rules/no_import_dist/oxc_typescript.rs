//! no-import-dist OXC backend — flag imports targeting `dist/` build output.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ImportDeclaration, ImportDeclarationSpecifier};
use std::sync::Arc;

pub struct Check;

/// Returns true if `spec` points into a `dist/` directory.
fn targets_dist(spec: &str) -> bool {
    spec.contains("/dist/") || spec.starts_with("dist/")
}

/// Returns true if the import has zero runtime impact: either a top-level
/// `import type { ... }` declaration, or a declaration where every named
/// specifier carries an inline `type` qualifier (`import { type A, type B }`).
/// Such imports pull nothing from the compiled artifact at runtime, so the
/// dist/ check (aimed at runtime use of build output) does not apply.
fn is_type_only(import: &ImportDeclaration) -> bool {
    if import.import_kind.is_type() {
        return true;
    }
    let Some(specifiers) = &import.specifiers else {
        return false;
    };
    let mut saw_named = false;
    for spec in specifiers {
        match spec {
            ImportDeclarationSpecifier::ImportSpecifier(named) => {
                saw_named = true;
                if !named.import_kind.is_type() {
                    return false;
                }
            }
            // A default or namespace specifier is always a value binding.
            _ => return false,
        }
    }
    saw_named
}

fn emit(ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>, spec: &str, offset: usize) {
    let (line, column) = byte_offset_to_line_col(ctx.source, offset);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "Import from '{spec}' targets `dist/`. Import from package entry point, not dist/."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration, AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ImportDeclaration(import) => {
                let spec = import.source.value.as_str();
                if targets_dist(spec) && !is_type_only(import) {
                    emit(ctx, diagnostics, spec, import.span.start as usize);
                }
            }
            AstKind::CallExpression(call) => {
                // require('pkg/dist/foo')
                let is_require = matches!(
                    &call.callee,
                    oxc_ast::ast::Expression::Identifier(id) if id.name.as_str() == "require"
                );
                if !is_require {
                    return;
                }
                let Some(first_arg) = call.arguments.first() else {
                    return;
                };
                let spec = match first_arg {
                    oxc_ast::ast::Argument::StringLiteral(s) => s.value.as_str(),
                    _ => return,
                };
                if targets_dist(spec) {
                    emit(ctx, diagnostics, spec, call.span.start as usize);
                }
            }
            _ => {}
        }
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Handle dynamic import() which is ImportExpression, not CallExpression
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::ImportExpression(import) = node.kind()
                && let oxc_ast::ast::Expression::StringLiteral(s) = &import.source {
                    let spec = s.value.as_str();
                    if targets_dist(spec) {
                        emit(ctx, &mut diagnostics, spec, import.span.start as usize);
                    }
                }
        }
        diagnostics
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            src,
            "/tmp/foo.ts",
            &crate::project::ProjectCtx::for_test_with_framework(""),
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    #[test]
    fn flags_value_import_from_dist() {
        let src = r#"import { foo } from "pkg/dist/foo";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_inline_type_import_from_dist() {
        // Issue #2074 exact example — AppType in the Next.js Pages Router is
        // only available via this internal dist/ path, as a type-only import.
        let src = r#"import { type AppType } from "next/dist/shared/lib/utils";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_top_level_type_import_from_dist() {
        let src = r#"import type { Foo } from "pkg/dist/foo";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_mixed_value_and_type_import_from_dist() {
        // Not every specifier is a type — a value binding still pulls runtime.
        let src = r#"import { foo, type Bar } from "pkg/dist/foo";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_default_import_from_dist() {
        // A default binding is always a value, even alongside inline types.
        let src = r#"import Foo, { type Bar } from "pkg/dist/foo";"#;
        assert_eq!(run(src).len(), 1);
    }
}
