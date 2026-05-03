//! regex-no-contradiction-with-assertion OXC backend.
//!
//! Flags patterns where a lookahead/lookbehind assertion contradicts
//! the adjacent element, e.g. `(?=a)b` or `(?!a)a`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn has_contradiction(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 4 < len {
        if bytes[i] == b'('
            && bytes[i + 1] == b'?'
            && bytes[i + 2] == b'='
            && bytes[i + 3] != b')'
            && bytes[i + 3] != b'\\'
            && let Some(close) = find_close_paren(bytes, i)
        {
            let after = close + 1;
            if after < len
                && bytes[after] != b'|'
                && bytes[after] != b')'
                && bytes[after] != b'('
                && bytes[i + 3] != bytes[after]
                && bytes[after].is_ascii_alphanumeric()
                && bytes[i + 3].is_ascii_alphanumeric()
            {
                return true;
            }
        }
        if bytes[i] == b'('
            && bytes[i + 1] == b'?'
            && bytes[i + 2] == b'!'
            && bytes[i + 3] != b')'
            && bytes[i + 3] != b'\\'
            && let Some(close) = find_close_paren(bytes, i)
        {
            let after = close + 1;
            if after < len && bytes[i + 3] == bytes[after] {
                return true;
            }
        }
        i += 1;
    }
    false
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
        let AstKind::RegExpLiteral(regex) = node.kind() else { return };

        let src = &ctx.source[regex.span.start as usize..regex.span.end as usize];
        // src is like /pattern/flags — strip leading / and trailing /flags
        let Some(pattern) = extract_pattern(src) else { return };

        if !has_contradiction(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, regex.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Assertion contradicts the pattern around it \u{2014} this branch can never match.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn extract_pattern(src: &str) -> Option<&str> {
    let src = src.strip_prefix('/')?;
    // Find last unescaped `/` outside character classes
    let bytes = src.as_bytes();
    let mut in_class = false;
    let mut i = 0;
    let mut last_slash = None;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => i += 2,
            b'[' => { in_class = true; i += 1; }
            b']' if in_class => { in_class = false; i += 1; }
            b'/' if !in_class => { last_slash = Some(i); i += 1; }
            _ => i += 1,
        }
    }
    last_slash.map(|pos| &src[..pos])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_positive_lookahead_contradiction() {
        assert_eq!(run_on(r#"const re = /(?=a)b/;"#).len(), 1);
    }

    #[test]
    fn flags_negative_lookahead_same_char() {
        assert_eq!(run_on(r#"const re = /(?!a)a/;"#).len(), 1);
    }

    #[test]
    fn allows_consistent_lookahead() {
        assert!(run_on(r#"const re = /(?=a)a/;"#).is_empty());
    }

    #[test]
    fn ignores_tailwind_class_string() {
        assert!(run_on(r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#).is_empty());
    }

    #[test]
    fn ignores_url_string() {
        assert!(run_on(r#"const u = "http://a/b/c";"#).is_empty());
    }

    #[test]
    fn ignores_import_path() {
        assert!(run_on(r#"import X from "@scope/pkg/sub";"#).is_empty());
    }
}
