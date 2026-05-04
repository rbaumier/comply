//! regex-no-potentially-useless-backreference OXC backend.
//!
//! Visits `RegExpLiteral` nodes only — never scans raw text.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Returns `true` when the regex `pattern` contains a backreference in a
/// different top-level alternative than the capturing group it references.
fn has_cross_alt_backref(pattern: &str) -> bool {
    let alts = split_top_level(pattern);
    if alts.len() < 2 {
        return false;
    }
    for (i, alt) in alts.iter().enumerate() {
        let bytes = alt.as_bytes();
        let mut k = 0;
        while k + 1 < bytes.len() {
            if bytes[k] == b'\\' && bytes[k + 1].is_ascii_digit() && bytes[k + 1] != b'0' {
                let group_num = (bytes[k + 1] - b'0') as usize;
                let mut group_count = 0;
                let mut found_in_other = false;
                for (j, other_alt) in alts.iter().enumerate() {
                    for &b in other_alt.as_bytes() {
                        if b == b'(' {
                            group_count += 1;
                            if group_count == group_num && j != i {
                                found_in_other = true;
                            }
                        }
                    }
                }
                if found_in_other {
                    return true;
                }
            }
            k += 1;
        }
    }
    false
}

/// Split a regex pattern on top-level `|` alternations, ignoring `|`
/// inside groups `(...)` and character classes `[...]`.
fn split_top_level(pattern: &str) -> Vec<&str> {
    let mut alts = Vec::new();
    let bytes = pattern.as_bytes();
    let mut depth = 0;
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\\' => {}
            b'(' | b'[' => depth += 1,
            b')' | b']' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            b'|' if depth == 0 => {
                alts.push(&pattern[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    alts.push(&pattern[start..]);
    alts
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
        let AstKind::RegExpLiteral(re) = node.kind() else {
            return;
        };
        let pattern = re.regex.pattern.text.as_str();
        if !has_cross_alt_backref(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Backreference may be useless \u{2014} some paths do not go through the referenced group.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
