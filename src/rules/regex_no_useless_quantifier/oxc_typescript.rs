//! OxcCheck backend for regex-no-useless-quantifier.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Detects useless quantifiers inside a regex pattern:
/// - `{1}` — matches exactly once anyway
/// - `{1,1}` — same
/// - Quantifier on an empty group `()+`, `()*`, `()?`, `(){...}`
fn has_useless_quantifier(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Respect escapes: `\{`, `\(` etc. are literals.
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }

        // Detect `{1}` or `{1,1}`.
        if bytes[i] == b'{' {
            let mut j = i + 1;
            let mut num_buf = String::new();
            while j < len && bytes[j].is_ascii_digit() {
                num_buf.push(bytes[j] as char);
                j += 1;
            }
            if j < len && bytes[j] == b'}' && num_buf == "1" {
                return true;
            } else if j < len && bytes[j] == b',' {
                j += 1;
                let mut num_buf2 = String::new();
                while j < len && bytes[j].is_ascii_digit() {
                    num_buf2.push(bytes[j] as char);
                    j += 1;
                }
                if j < len && bytes[j] == b'}' && num_buf == "1" && num_buf2 == "1" {
                    return true;
                }
            }
        }

        // Detect quantifier on empty group: ()+, ()*, ()?, (){...}.
        if bytes[i] == b'(' && i + 2 < len && bytes[i + 1] == b')' {
            let after = bytes[i + 2];
            if after == b'+' || after == b'*' || after == b'?' || after == b'{' {
                return true;
            }
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
        if !has_useless_quantifier(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Useless quantifier \u{2014} it can only match once or matches an empty element.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
