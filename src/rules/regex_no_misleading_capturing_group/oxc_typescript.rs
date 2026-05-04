//! regex-no-misleading-capturing-group OXC backend — visits RegExpLiteral
//! nodes only.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Detects a capturing group containing alternation (`|`) immediately
/// followed by a quantifier (`+`, `*`, `?`, `{…}`).
fn has_misleading_capture(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'(' && i + 1 < len && bytes[i + 1] != b'?' {
            let mut depth = 1;
            let mut j = i + 1;
            let mut has_alternation = false;
            while j < len && depth > 0 {
                match bytes[j] {
                    b'\\' => j += 1,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'|' if depth == 1 => has_alternation = true,
                    _ => {}
                }
                j += 1;
            }
            if depth == 0 && has_alternation && j + 1 < len {
                let next = bytes[j + 1];
                if matches!(next, b'+' | b'*' | b'?' | b'{') {
                    return true;
                }
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
        let AstKind::RegExpLiteral(re) = node.kind() else {
            return;
        };
        let pattern = re.regex.pattern.text.as_str();
        if !has_misleading_capture(pattern.as_ref()) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Capturing group with alternation and quantifier is misleading \u{2014} the capture may match different things.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
