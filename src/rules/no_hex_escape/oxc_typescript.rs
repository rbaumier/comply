//! no-hex-escape OXC backend — flag `\xNN` hex escapes, prefer `\u00NN`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn find_hex_escapes(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut hits = Vec::new();

    while i + 3 < len {
        if bytes[i] == b'\\' {
            let bs_start = i;
            while i < len && bytes[i] == b'\\' {
                i += 1;
            }
            let bs_count = i - bs_start;

            if bs_count % 2 == 1
                && i < len
                && bytes[i] == b'x'
                && i + 2 < len
                && bytes[i + 1].is_ascii_hexdigit()
                && bytes[i + 2].is_ascii_hexdigit()
            {
                let hex = &text[i + 1..i + 3];
                hits.push(hex.to_string());
                i += 3;
            }
        } else {
            i += 1;
        }
    }
    hits
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TemplateLiteral(tpl) = node.kind() else {
            return;
        };
        let raw = &ctx.source[tpl.span.start as usize..tpl.span.end as usize];
        for hex in find_hex_escapes(raw) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, tpl.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Use Unicode escape `\\u00{hex}` instead of hex escape `\\x{hex}`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Also check string literals (no AstType for StringLiteral).
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::StringLiteral(lit) = node.kind() else {
                continue;
            };
            let raw = &ctx.source[lit.span.start as usize..lit.span.end as usize];
            for hex in find_hex_escapes(raw) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, lit.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Use Unicode escape `\\u00{hex}` instead of hex escape `\\x{hex}`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}
