//! OxcCheck backend for ts-no-export-equal — flag CommonJS-style `export = X`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSExportAssignment]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSExportAssignment(export) = node.kind() else { return };

        let (line, column) = byte_offset_to_line_col(ctx.source, export.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "CommonJS-style `export = ...` — use `export default` or named exports."
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
    fn flags_export_equal_value() {
        let d = run_on("const x = 1;\nexport = x;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("export = "));
    }


    #[test]
    fn flags_export_equal_class() {
        let d = run_on("class Foo {}\nexport = Foo;");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_export_default() {
        assert!(run_on("const x = 1;\nexport default x;").is_empty());
    }


    #[test]
    fn allows_named_export() {
        assert!(run_on("export const x = 1;").is_empty());
    }
}
