//! regex-no-escape-backspace OXC backend.
//!
//! Flags `[\b]` (backspace escape) inside a character class of a regex
//! literal. Inside `[...]`, `\b` means backspace (U+0008), not a word
//! boundary — almost always a mistake.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

fn has_backspace_in_char_class(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'[' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b']' {
                if bytes[j] == b'\\' && j + 1 < bytes.len() && bytes[j + 1] == b'b' {
                    return true;
                }
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                j += 1;
            }
            i = j;
        }
        i += 1;
    }
    false
}

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
        let AstKind::RegExpLiteral(regex) = node.kind() else { return };

        let pattern = regex.regex.pattern.text.as_str();
        if !has_backspace_in_char_class(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, regex.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`[\\b]` matches backspace, not a word boundary \u{2014} use `\\b` outside a character class for word boundaries.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
