//! OxcCheck backend for ts-no-empty-object-type — flag `{}` used as a type.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeLiteral(lit) = node.kind() else { return };
        if !lit.members.is_empty() {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`{}` as a type matches any non-nullish value. \
                      Use `Record<string, never>` for an empty object, \
                      or `object` / `unknown`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
