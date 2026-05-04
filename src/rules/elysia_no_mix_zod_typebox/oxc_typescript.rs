//! OxcCheck backend — flag mixing Zod with Elysia's `t`.

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
        Some(&["zod"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::ImportDeclaration(import) = node.kind() else { return };

        let source_value = import.source.value.as_str();
        if source_value != "zod" {
            return;
        }

        let uses_t = ctx.source.contains("t.Object(")
            || ctx.source.contains("t.String(")
            || ctx.source.contains("t.Number(")
            || ctx.source.contains("t.Array(")
            || ctx.source.contains("t.Boolean(");
        if !uses_t {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "File uses both Zod and Elysia's `t` validators — pick one. Mixing breaks Elysia's static type inference.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
