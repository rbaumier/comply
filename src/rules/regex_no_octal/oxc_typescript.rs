//! regex-no-octal OXC backend.
//!
//! Visits OXC `RegExpLiteral` nodes only — never scans raw text — so
//! URLs, Tailwind arbitrary-value classes, and scoped import paths
//! inside string literals cannot false-positive as regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn has_octal_escape(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'\\' {
            let mut c = 0;
            let mut j = i;
            while j < len && bytes[j] == b'\\' {
                c += 1;
                j += 1;
            }
            if c % 2 == 1 {
                let after = i + c;
                if after < len
                    && bytes[after].is_ascii_digit()
                    && bytes[after] != b'8'
                    && bytes[after] != b'9'
                {
                    if bytes[after] == b'0' {
                        if after + 1 < len && bytes[after + 1] >= b'0' && bytes[after + 1] <= b'7'
                        {
                            return true;
                        }
                    } else {
                        return true;
                    }
                }
            }
            i += c;
        } else {
            i += 1;
        }
    }
    false
}

fn extract_pattern(src: &str) -> Option<&str> {
    let src = src.strip_prefix('/')?;
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
        let Some(pattern) = extract_pattern(src) else { return };

        if !has_octal_escape(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, regex.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Octal escape in regex is ambiguous \u{2014} use a named backreference or Unicode escape instead.".into(),
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
    fn flags_octal_escape_in_regex() {
        assert_eq!(run_on(r#"const re = /\1/;"#).len(), 1);
    }

    #[test]
    fn flags_multi_digit_octal() {
        assert_eq!(run_on(r#"const re = /\12/;"#).len(), 1);
    }

    #[test]
    fn allows_null_escape() {
        assert!(run_on(r#"const re = /\0/;"#).is_empty());
    }

    #[test]
    fn flags_octal_after_null() {
        assert_eq!(run_on(r#"const re = /\00/;"#).len(), 1);
    }

    #[test]
    fn allows_no_regex() {
        assert!(run_on("const x = 42;").is_empty());
    }

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/b\\1";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }
}
