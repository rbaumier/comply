//! ts-consistent-type-exports oxc backend — flag `export { type A, type B }`
//! where every specifier uses inline `type`; prefer `export type { A, B }`.

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
        let AstKind::ExportNamedDeclaration(export) = node.kind() else { return };

        // Already a top-level `export type { ... }` — fine.
        if export.export_kind.is_type() {
            return;
        }

        // Must have specifiers (named export, not `export const ...`).
        let specs = &export.specifiers;
        if specs.is_empty() {
            return;
        }

        // Check if ALL specifiers use inline `type`.
        if !specs.iter().all(|s| s.export_kind.is_type()) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, export.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "All exported specifiers are types — use `export type { ... }` \
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
        let d = run_on("export { type Foo, type Bar };");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_inline_type_reexport() {
        let d = run_on("export { type Foo } from './baz';");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_export_type() {
        assert!(run_on("export type { Foo } from './baz';").is_empty());
    }


    #[test]
    fn allows_mixed_value_and_type() {
        assert!(run_on("export { Foo, type Bar } from './baz';").is_empty());
    }


    #[test]
    fn allows_plain_value_export() {
        assert!(run_on("export { foo } from './baz';").is_empty());
    }
}
