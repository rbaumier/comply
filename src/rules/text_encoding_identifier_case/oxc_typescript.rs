//! text-encoding-identifier-case OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// Known encoding identifiers and their canonical lowercase form.
const ENCODINGS: &[(&str, &str)] = &[
    ("UTF-8", "utf-8"),
    ("Utf-8", "utf-8"),
    ("UTF8", "utf8"),
    ("Utf8", "utf8"),
    ("ASCII", "ascii"),
    ("Ascii", "ascii"),
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StringLiteral(lit) = node.kind() else { return };
        let content = lit.value.as_str();

        for &(bad, good) in ENCODINGS {
            if content == bad {
                let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Prefer `'{good}'` over `'{bad}'`."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
