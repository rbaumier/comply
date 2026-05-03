//! regex-use-unicode-flag OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::RegExpFlags;
use std::sync::Arc;

pub struct Check;

/// Returns true if the regex pattern contains a `\p{...}` or `\P{...}`
/// Unicode property escape (respecting backslash escaping).
fn has_unicode_property_escape(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if (next == b'p' || next == b'P') && i + 2 < bytes.len() && bytes[i + 2] == b'{' {
                return true;
            }
            i += 2;
            continue;
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
        let AstKind::RegExpLiteral(re) = node.kind() else { return };

        let pattern = re.regex.pattern.text.as_str();
        if !has_unicode_property_escape(pattern) {
            return;
        }
        if re.regex.flags.contains(RegExpFlags::U) || re.regex.flags.contains(RegExpFlags::V) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unicode property escape (`\\p{...}`) requires the `u` or `v` flag.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
