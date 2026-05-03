//! prefer-type-guard oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

/// Check if a function body source contains `typeof` or `instanceof`.
fn body_has_type_check(source: &str, start: usize, end: usize) -> bool {
    let slice = &source[start..end];
    slice.contains("typeof ") || slice.contains("instanceof ")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Function(func) = node.kind() else {
            return;
        };

        // Must have a name starting with "is" + uppercase.
        let Some(id) = &func.id else { return };
        let name = id.name.as_str();
        if !name.starts_with("is") {
            return;
        }
        let after_is = &name[2..];
        if after_is.is_empty() || !after_is.starts_with(|c: char| c.is_ascii_uppercase()) {
            return;
        }

        // Return type must be `: boolean` (not a type predicate).
        let Some(ret) = &func.return_type else { return };
        let rt_span = ret.span;
        let rt_text = &ctx.source[rt_span.start as usize..rt_span.end as usize];
        let rt_inner = rt_text.trim().strip_prefix(':').unwrap_or(rt_text.trim()).trim();
        if rt_inner != "boolean" {
            return;
        }

        // Check body for typeof / instanceof.
        let Some(body) = &func.body else { return };
        if !body_has_type_check(ctx.source, body.span.start as usize, body.span.end as usize) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, func.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Function `isX` returns `boolean` with type checks \u{2014} use a type predicate (`x is Type`) instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
