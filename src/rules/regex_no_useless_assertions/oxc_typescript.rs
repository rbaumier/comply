//! regex-no-useless-assertions OxcCheck backend.
//!
//! Visits `RegExpLiteral` nodes only — string literals containing `^` or `$`
//! (URLs, scoped imports, Tailwind values) cannot be mistaken for regex.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn has_useless_dollar(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'$' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next != b')' && next != b'|'
                && (i == 0 || bytes[i - 1] != b'\\') {
                    return true;
                }
        }
    }
    false
}

fn has_useless_caret(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'^' && i > 0 {
            let prev = bytes[i - 1];
            if prev != b'(' && prev != b'|' && prev != b'[' && prev != b'\\' {
                return true;
            }
        }
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
        let AstKind::RegExpLiteral(re) = node.kind() else { return };

        if re.regex.flags.contains(oxc_ast::ast::RegExpFlags::M) {
            return;
        }

        let pattern = re.regex.pattern.text.as_str();

        if !has_useless_dollar(pattern) && !has_useless_caret(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Assertion is always true or always false and has no effect.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
