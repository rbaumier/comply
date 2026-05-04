//! regex-no-trivially-nested-assertion OXC backend.
//!
//! Visits `RegExpLiteral` nodes only — never scans raw text.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Returns the offsets (within `pattern`) of lookaround assertions that
/// trivially nest another lookaround of the same kind.
fn find_trivially_nested(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 4 < len {
        if bytes[i] == b'('
            && bytes[i + 1] == b'?'
            && let Some(kind) = get_lookaround_kind(bytes, i)
        {
            let content_start = i + kind.len() + 2;
            let mut j = content_start;
            let mut depth = 1;
            while j + 3 < len && depth > 0 {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'(' && bytes[j + 1] == b'?' {
                    if let Some(inner_kind) = get_lookaround_kind(bytes, j)
                        && inner_kind == kind {
                            return true;
                        }
                    depth += 1;
                } else if bytes[j] == b'(' {
                    depth += 1;
                } else if bytes[j] == b')' {
                    depth -= 1;
                }
                j += 1;
            }
        }
        i += 1;
    }
    false
}

fn get_lookaround_kind(bytes: &[u8], pos: usize) -> Option<&'static str> {
    if pos + 3 > bytes.len() || bytes[pos] != b'(' || bytes[pos + 1] != b'?' {
        return None;
    }
    match bytes[pos + 2] {
        b'=' => Some("="),
        b'!' => Some("!"),
        b'<' if pos + 4 <= bytes.len() => match bytes[pos + 3] {
            b'=' => Some("<="),
            b'!' => Some("<!"),
            _ => None,
        },
        _ => None,
    }
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
        if !find_trivially_nested(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message:
                "Trivially nested lookaround assertion \u{2014} merge with parent or simplify."
                    .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
