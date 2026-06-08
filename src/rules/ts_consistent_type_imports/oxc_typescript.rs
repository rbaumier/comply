//! ts-consistent-type-imports oxc backend — flag `import { type A, type B }`
//! where every named specifier uses inline `type`; prefer `import type { A, B }`.

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
        let AstKind::ImportDeclaration(import) = node.kind() else { return };

        // Already a top-level type import — fine.
        if import.import_kind.is_type() {
            return;
        }

        let Some(specifiers) = &import.specifiers else { return };
        if specifiers.is_empty() {
            return;
        }

        // Only consider named specifiers (skip default/namespace).
        let named: Vec<_> = specifiers
            .iter()
            .filter(|s| matches!(s, ImportDeclarationSpecifier::ImportSpecifier(_)))
            .collect();

        if named.is_empty() {
            return;
        }

        // All named specifiers must be inline `type`.
        let all_type = named.iter().all(|s| {
            if let ImportDeclarationSpecifier::ImportSpecifier(spec) = s {
                spec.import_kind.is_type()
            } else {
                false
            }
        });

        if !all_type {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "All imported specifiers are types — use `import type { ... }` \
                      at the top level instead of inline `type` markers."
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
        let d = run_on("import { type Foo, type Bar } from 'baz';");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_single_inline_type() {
        let d = run_on("import { type Foo } from 'baz';");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_import_type() {
        assert!(run_on("import type { Foo } from 'baz';").is_empty());
    }


    #[test]
    fn allows_mixed_value_and_type() {
        assert!(run_on("import { Foo, type Bar } from 'baz';").is_empty());
    }


    #[test]
    fn allows_plain_value_import() {
        assert!(run_on("import { foo } from 'baz';").is_empty());
    }
}
