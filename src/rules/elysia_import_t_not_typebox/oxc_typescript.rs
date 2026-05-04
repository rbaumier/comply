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
        if !ctx.source.contains("@sinclair/typebox") {
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
