use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportNamedDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // `import {} from 'x'` is a valid side-effect import (runs the module's
        // top-level code, TS verifies the path) — equivalent to `import 'x'` — so it
        // is not flagged; only empty-specifier re-exports are.
        let AstKind::ExportNamedDeclaration(export) = node.kind() else {
            return;
        };
        // Flag `export {} from './module'` — re-export with empty specifiers.
        // Only flag when there's a source (bare `export {}` is weird but not re-export).
        if export.source.is_none() {
            return;
        }
        if export.specifiers.is_empty() && export.declaration.is_none() {
            let (line, column) = byte_offset_to_line_col(ctx.source, export.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "export statement with empty specifiers `{}` is not \
                     allowed \u{2014} add specifiers, use a side-effect import, or \
                     remove the statement."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // `import {} from 'x'` is a valid side-effect import — never flagged (#2142).
    #[test]
    fn allows_empty_import_deno_npm_specifier() {
        assert!(run_on(r#"import {} from "npm:chalk";"#).is_empty());
    }

    #[test]
    fn allows_empty_import_deno_jsr_specifier() {
        assert!(run_on(r#"import {} from "jsr:@std/assert";"#).is_empty());
    }

    #[test]
    fn allows_empty_import_relative_specifier() {
        assert!(run_on(r#"import {} from "./vendor2.ts";"#).is_empty());
    }

    #[test]
    fn allows_import_with_specifiers() {
        assert!(run_on(r#"import { x } from "./y";"#).is_empty());
    }

    #[test]
    fn allows_side_effect_import() {
        assert!(run_on(r#"import "./y";"#).is_empty());
    }

    // Empty-specifier re-exports are still flagged.
    #[test]
    fn flags_export_with_empty_specifiers() {
        let diags = run_on(r#"export {} from "./mod";"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("export"));
    }
}
