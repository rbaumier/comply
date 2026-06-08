//! elysia-import-t-not-typebox oxc backend — flag direct TypeBox imports in Elysia files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@sinclair/typebox"])
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
        if !ctx.project.has_framework("elysia") {
            return;
        }
        if !ctx.source_contains("@sinclair/typebox") {
            return;
        }
        if import.source.value.as_str() != "@sinclair/typebox" {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Import `t` from `elysia` instead of `Type` from `@sinclair/typebox` — Elysia ships augmented validators.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_typebox_import_in_elysia_file() {
        let src = "import { Elysia } from 'elysia';\nimport { Type } from '@sinclair/typebox';\n";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_t_from_elysia() {
        let src = "import { Elysia, t } from 'elysia';\n";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_typebox_outside_elysia_files() {
        let src = "import { Type } from '@sinclair/typebox';\n";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
