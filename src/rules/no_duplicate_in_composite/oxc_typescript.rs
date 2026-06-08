//! OxcCheck backend — flag duplicate types in union (`|`) or intersection (`&`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use oxc_span::GetSpan;
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

fn check_members(types: &oxc_allocator::Vec<'_, TSType<'_>>, source: &str, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>, span_start: u32) {
    if types.len() < 2 {
        return;
    }

    let mut seen = FxHashSet::default();
    for ty in types.iter() {
        let text = &source[ty.span().start as usize..ty.span().end as usize];
        let normalized = text.trim();
        if !normalized.is_empty() && !seen.insert(normalized.to_string()) {
            let (line, column) = byte_offset_to_line_col(source, span_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Duplicate type in composite — remove the repeated member.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return; // one diagnostic per composite
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSUnionType, AstType::TSIntersectionType]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::TSUnionType(union) => {
                check_members(&union.types, ctx.source, ctx, diagnostics, union.span.start);
            }
            AstKind::TSIntersectionType(inter) => {
                check_members(&inter.types, ctx.source, ctx, diagnostics, inter.span.start);
            }
            _ => {}
        }
    }
}
