//! no-mocks-import oxc backend — flag imports that reference a `__mocks__` directory.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
        let spec = import.source.value.as_str();
        if !spec.contains("__mocks__") {
            return;
        }
        let raw = &ctx.source[import.source.span.start as usize..import.source.span.end as usize];
        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Import from {raw} references `__mocks__`. Let Jest/Vitest auto-resolve mocks, don't import from __mocks__ directly."
            ),
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
    fn flags_relative_mocks_import() {
        let d = run_on("import foo from './__mocks__/foo';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("__mocks__"));
    }


    #[test]
    fn flags_nested_mocks_import() {
        let d = run_on("import bar from '../utils/__mocks__/bar';");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_package_mocks_import() {
        let d = run_on("import baz from 'pkg/__mocks__/baz';");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_normal_relative_import() {
        assert!(run_on("import foo from './foo';").is_empty());
    }


    #[test]
    fn allows_normal_package_import() {
        assert!(run_on("import foo from 'pkg';").is_empty());
    }
}
