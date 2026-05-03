//! regex-no-empty-group OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn has_empty_group(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'(' && bytes[i + 1] == b')' {
            return true;
        }
        i += 1;
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(regexp) = node.kind() else {
            return;
        };
        let pattern = regexp.regex.pattern.text.as_str();
        if !has_empty_group(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, regexp.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Empty capturing group `()` in regex \u{2014} add a pattern or remove it."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
