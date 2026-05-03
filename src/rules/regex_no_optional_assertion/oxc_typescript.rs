//! regex-no-optional-assertion OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// Scans a regex pattern for assertions (`^`, `$`, `(?=...)`, `(?!...)`,
/// `(?<=...)`, `(?<!...)`) inside a group whose quantifier is `?` or `*`
/// (i.e. the group may match zero times, making the assertion a no-op).
fn has_optional_assertion(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'(' {
            let mut depth = 1;
            let mut j = i + 1;
            let mut has_assertion = false;
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
                    b'^' | b'$' => {
                        if depth == 1 {
                            has_assertion = true;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            // Check for lookaround `(?=...)`, `(?!...)`, `(?<=...)`, `(?<!...)`
            // anywhere inside the group.
            if !has_assertion {
                let mut k = i + 1;
                while k + 2 < j {
                    if bytes[k] == b'(' && bytes[k + 1] == b'?' {
                        let c = bytes[k + 2];
                        if c == b'=' || c == b'!' {
                            has_assertion = true;
                            break;
                        }
                        if c == b'<' && k + 3 < j {
                            let d = bytes[k + 3];
                            if d == b'=' || d == b'!' {
                                has_assertion = true;
                                break;
                            }
                        }
                    }
                    k += 1;
                }
            }
            if depth == 0 && has_assertion && j + 1 < len {
                let next = bytes[j + 1];
                if next == b'?' || next == b'*' {
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
        let AstKind::RegExpLiteral(regexp) = node.kind() else {
            return;
        };
        let pattern = regexp.regex.pattern.text.as_str();
        if !has_optional_assertion(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, regexp.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Assertion inside an optional group is effectively ignored.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
