//! number-literal-case — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

/// The canonical form: lowercase prefix/exponent, uppercase hex digits.
fn canonical(raw: &str) -> Option<String> {
    let (body, suffix) = if let Some(stripped) = raw.strip_suffix('n') {
        (stripped, "n")
    } else {
        (raw, "")
    };

    if body.len() < 2 {
        return None;
    }

    let prefix_lower = body[..2].to_lowercase();
    let fixed = match prefix_lower.as_str() {
        "0x" => {
            let digits = &body[2..];
            format!("0x{}{}", digits.to_uppercase(), suffix)
        }
        "0b" | "0o" => {
            format!("{}{}{}", prefix_lower, &body[2..], suffix)
        }
        _ => {
            if !body.contains('E') && !body.contains('e') {
                return None;
            }
            let lowered = body.to_lowercase();
            format!("{}{}", lowered, suffix)
        }
    };

    if fixed == raw { None } else { Some(fixed) }
}

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NumericLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let oxc_ast::AstKind::NumericLiteral(lit) = node.kind() else {
            return;
        };
        let span = lit.span();
        let raw = &semantic.source_text()[span.start as usize..span.end as usize];
        if let Some(fixed) = canonical(raw) {
            let (line, col) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: col,
                rule_id: super::META.id.into(),
                message: format!(
                    "Invalid number literal casing: `{}` should be `{}`.",
                    raw, fixed
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
