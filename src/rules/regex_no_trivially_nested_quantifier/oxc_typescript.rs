//! regex-no-trivially-nested-quantifier OxcCheck backend.
//!
//! Visits `RegExpLiteral` nodes only — never scans raw text — so
//! parenthesised strings cannot false-positive.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Detects trivially nested quantifiers that can be merged.
fn find_trivially_nested_quantifiers(pattern: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'(' && i + 2 < len && bytes[i + 1] == b'?' && bytes[i + 2] == b':' {
            let group_start = i;
            let content_start = i + 3;
            let mut depth = 1;
            let mut j = content_start;

            while j < len && depth > 0 {
                match bytes[j] {
                    b'\\' => j += 1,
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            let close = j - 1;
            if depth == 0 {
                let content = &pattern[content_start..close];
                let has_inner_quantifier = is_single_quantified_element(content);
                if has_inner_quantifier && close + 1 < len {
                    let next = bytes[close + 1];
                    if next == b'+' || next == b'*' || next == b'?' || next == b'{' {
                        hits.push(group_start);
                    }
                }
            }
        }
        i += 1;
    }
    hits
}

fn is_single_quantified_element(content: &str) -> bool {
    let bytes = content.as_bytes();
    let clen = bytes.len();
    if clen < 2 {
        return false;
    }

    let elem_len;
    if bytes[0] == b'\\' {
        elem_len = 2;
    } else if bytes[0] == b'[' {
        if let Some(close) = find_char_class_close(bytes, 0) {
            elem_len = close + 1;
        } else {
            return false;
        }
    } else if bytes[0] == b'.' || bytes[0].is_ascii_alphanumeric() {
        elem_len = 1;
    } else {
        return false;
    }

    if elem_len >= clen {
        return false;
    }

    let rest = bytes[elem_len];
    match rest {
        b'+' | b'*' | b'?' => {
            elem_len + 1 == clen || (elem_len + 2 == clen && bytes[elem_len + 1] == b'?')
        }
        b'{' => bytes[elem_len..].contains(&b'}'),
        _ => false,
    }
}

fn find_char_class_close(bytes: &[u8], start: usize) -> Option<usize> {
    let mut j = start + 1;
    if j < bytes.len() && bytes[j] == b'^' {
        j += 1;
    }
    if j < bytes.len() && bytes[j] == b']' {
        j += 1;
    }
    while j < bytes.len() {
        if bytes[j] == b'\\' {
            j += 2;
            continue;
        }
        if bytes[j] == b']' {
            return Some(j);
        }
        j += 1;
    }
    None
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
        if find_trivially_nested_quantifiers(pattern).is_empty() {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Trivially nested quantifiers can be merged into a single quantifier.".into(),
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
    fn flags_nested_plus_optional() {
        assert_eq!(run_on(r#"const re = /(?:a+)?/;"#).len(), 1);
    }


    #[test]
    fn allows_multi_element_group() {
        assert!(run_on(r#"const re = /(?:ab)+/;"#).is_empty());
    }


    #[test]
    fn flags_nested_star_plus() {
        assert_eq!(run_on(r#"const re = /(?:a*)+/;"#).len(), 1);
    }


    #[test]
    fn ignores_quantifier_lookalike_in_tailwind_string() {
        let src = r#"const x = "has-[(?:a+)?]:block";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_quantifier_lookalike_in_url() {
        let src = r#"const u = "http://ex/(?:a+)?";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_empty_scoped_import_path() {
        let src = r#"import X from "@scope/(?:a+)?";"#;
        assert!(run_on(src).is_empty());
    }
}
