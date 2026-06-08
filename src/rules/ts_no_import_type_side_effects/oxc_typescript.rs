//! ts-no-import-type-side-effects OXC backend — flag `import { type A, type B }`
//! where every specifier has an inline `type` qualifier but the import
//! itself lacks a top-level `type` keyword.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::ImportDeclarationSpecifier;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };

        // Already a top-level `import type { ... }` — nothing to flag.
        if import.import_kind.is_type() {
            return;
        }

        let Some(specifiers) = &import.specifiers else {
            return;
        };

        // Collect only named specifiers (ImportSpecifier).
        let named: Vec<_> = specifiers
            .iter()
            .filter_map(|s| {
                if let ImportDeclarationSpecifier::ImportSpecifier(spec) = s {
                    Some(spec)
                } else {
                    None
                }
            })
            .collect();

        if named.is_empty() {
            return;
        }

        // Check every named specifier has an inline `type` qualifier.
        let all_type = named.iter().all(|spec| spec.import_kind.is_type());

        if !all_type {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "All specifiers have inline `type` qualifiers \u{2014} use a \
                      top-level `import type` to avoid a runtime side-effect import."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_all_inline_type_specifiers() {
        let diags = run_on("import { type A, type B } from 'mod';");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_top_level_import_type() {
        assert!(run_on("import type { A, B } from 'mod';").is_empty());
    }


    #[test]
    fn allows_mixed_specifiers() {
        assert!(run_on("import { type A, B } from 'mod';").is_empty());
    }
}
