//! regex-no-useless-backreference OXC backend — flag backreferences that
//! always resolve to the empty string (forward or self references).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// Returns `true` when `pattern` contains a backreference that always
/// resolves to the empty string.
fn has_useless_backref(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut group_count = 0usize;
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            if bytes[i + 1].is_ascii_digit() && bytes[i + 1] != b'0' {
                let ref_num = (bytes[i + 1] - b'0') as usize;
                if ref_num > group_count {
                    return true;
                }
            }
            i += 2;
            continue;
        }
        if bytes[i] == b'(' && (i + 1 >= bytes.len() || bytes[i + 1] != b'?') {
            group_count += 1;
            let inner_start = i + 1;
            if inner_start + 1 < bytes.len()
                && bytes[inner_start] == b'\\'
                && bytes[inner_start + 1].is_ascii_digit()
            {
                let ref_num = (bytes[inner_start + 1] - b'0') as usize;
                if ref_num == group_count {
                    return true;
                }
            }
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
        let AstKind::RegExpLiteral(re) = node.kind() else {
            return;
        };
        let pattern = re.regex.pattern.text.as_str();
        if !has_useless_backref(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message:
                "Backreference always resolves to the empty string \u{2014} it references itself or a forward group."
                    .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
