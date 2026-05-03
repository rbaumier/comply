//! regex-no-duplicate-chars OXC backend — visit `RegExpLiteral` nodes only.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Scans a regex pattern for `[...]` character classes containing
/// duplicate single characters (e.g. `[aab]`).
fn has_duplicate_in_char_class(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
            if backslashes % 2 != 0 {
                i += 1;
                continue;
            }
            let start = i + 1;
            let content_start = if start < bytes.len() && bytes[start] == b'^' {
                start + 1
            } else {
                start
            };
            let mut j = start;
            if j < bytes.len() && bytes[j] == b']' {
                j += 1;
            }
            while j < bytes.len() && bytes[j] != b']' {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                j += 1;
            }
            if j < bytes.len() {
                let content = &pattern[content_start..j];
                let mut chars: Vec<char> = Vec::new();
                let mut ci = 0;
                let cbytes = content.as_bytes();
                while ci < cbytes.len() {
                    if cbytes[ci] == b'\\' {
                        ci += 2;
                        continue;
                    }
                    if ci + 1 < cbytes.len() && cbytes[ci + 1] == b'-' {
                        ci += 3;
                        continue;
                    }
                    chars.push(cbytes[ci] as char);
                    ci += 1;
                }
                let len_before = chars.len();
                chars.sort_unstable();
                chars.dedup();
                if chars.len() < len_before {
                    return true;
                }
            }
            i = j + 1;
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

        let pattern = &ctx.source[re.span.start as usize..re.span.end as usize];
        // Strip surrounding `/pattern/flags`.
        let Some(inner) = pattern.strip_prefix('/') else { return };
        let Some(last_slash) = inner.rfind('/') else { return };
        let pat = &inner[..last_slash];

        if !has_duplicate_in_char_class(pat) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Duplicate character in regex character class \u{2014} remove the redundant character.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
