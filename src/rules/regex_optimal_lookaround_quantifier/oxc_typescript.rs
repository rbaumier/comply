//! regex-optimal-lookaround-quantifier OxcCheck backend.
//!
//! Visits `RegExpLiteral` nodes — never scans raw text.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

fn is_quantifier(b: u8) -> bool {
    b == b'+' || b == b'*'
}

fn find_close_paren(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 1;
    let mut j = start + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j += 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

fn has_suboptimal_lookaround_quantifier(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 4 < len {
        if bytes[i] == b'(' && bytes[i + 1] == b'?' {
            let is_lookahead = bytes[i + 2] == b'=' || bytes[i + 2] == b'!';
            let is_lookbehind = bytes[i + 2] == b'<'
                && i + 3 < len
                && (bytes[i + 3] == b'=' || bytes[i + 3] == b'!');

            if is_lookahead || is_lookbehind {
                let content_start = if is_lookbehind { i + 4 } else { i + 3 };
                if let Some(close) = find_close_paren(bytes, i) {
                    let cbytes = &bytes[content_start..close];

                    if is_lookahead {
                        let clen = cbytes.len();
                        if clen > 0 && is_quantifier(cbytes[clen - 1]) {
                            return true;
                        }
                    } else {
                        if cbytes.len() > 1 && is_quantifier(cbytes[1]) {
                            return true;
                        }
                    }
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
        if !has_suboptimal_lookaround_quantifier(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "regex-optimal-lookaround-quantifier".into(),
            message: "Quantifier at the edge of a lookaround is misleading \u{2014} it should match a constant number of times.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_quantifier_in_lookahead() {
        assert_eq!(run_on(r#"const re = /(?=a+)/;"#).len(), 1);
    }


    #[test]
    fn allows_no_quantifier_in_lookahead() {
        assert!(run_on(r#"const re = /(?=a)/;"#).is_empty());
    }


    #[test]
    fn flags_star_in_negative_lookahead() {
        assert_eq!(run_on(r#"const re = /(?!a*)/;"#).len(), 1);
    }


    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[(?=a+)]:grid";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/(?=b+)/c";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_scoped_import_empty() {
        let src = r#"import "";"#;
        assert!(run_on(src).is_empty());
    }
}
