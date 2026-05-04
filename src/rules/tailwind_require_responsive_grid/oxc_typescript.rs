//! tailwind-require-responsive-grid OXC backend — flag `grid-cols-2+` without
//! a responsive variant.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const BREAKPOINTS: &[&str] = &["sm:", "md:", "lg:", "xl:", "2xl:"];

fn cols_count(tok: &str) -> Option<u32> {
    tok.strip_prefix("grid-cols-")?.parse::<u32>().ok()
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["grid-cols-"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else { return };
        let oxc_ast::ast::JSXAttributeName::Identifier(ident) = &attr.name else { return };
        let name = ident.name.as_str();
        if name != "className" && name != "class" {
            return;
        }
        let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
            return;
        };
        let value = lit.value.as_str();

        let mut base_cols: Option<u32> = None;
        let mut has_responsive_cols = false;

        for tok in value.split_whitespace() {
            if let Some(bp) = BREAKPOINTS.iter().find(|bp| tok.starts_with(**bp)) {
                let after = &tok[bp.len()..];
                if after.starts_with("grid-cols-") {
                    has_responsive_cols = true;
                }
                continue;
            }
            if let Some(n) = cols_count(tok) {
                base_cols = Some(n);
            }
        }

        let Some(base) = base_cols else { return };
        if base < 2 {
            return;
        }
        if has_responsive_cols {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`grid-cols-2+` without a mobile-first fallback. Prefer `grid-cols-1 md:grid-cols-N` so the grid collapses on small screens.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
