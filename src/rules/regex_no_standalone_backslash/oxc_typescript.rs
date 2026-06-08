//! regex-no-standalone-backslash OXC backend — flag identity escapes in regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// Characters that are valid after a backslash in regex.
const VALID_AFTER_BACKSLASH: &[u8] = b"dDwWsSnrtfvbB0kpPuxc.*+?^${}[]()|\\/123456789";

fn has_standalone_backslash(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len.saturating_sub(1) {
        if bytes[i] == b'\\' {
            let next = bytes[i + 1];
            if next == b'\\' {
                i += 2;
                continue;
            }
            if !VALID_AFTER_BACKSLASH.contains(&next) && next.is_ascii_alphabetic() {
                return true;
            }
            i += 2;
        } else {
            i += 1;
        }
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
        let AstKind::RegExpLiteral(re) = node.kind() else {
            return;
        };
        let pattern = re.regex.pattern.text.as_str();
        if !has_standalone_backslash(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message:
                "Backslash followed by non-special character is an identity escape \u{2014} likely a mistake."
                    .into(),
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
    fn flags_backslash_before_normal_letter() {
        // \a is not a valid regex escape
        assert_eq!(run_on(r#"const re = /\a/;"#).len(), 1);
    }


    #[test]
    fn flags_backslash_e() {
        assert_eq!(run_on(r#"const re = /\e/;"#).len(), 1);
    }


    #[test]
    fn allows_valid_escape_d() {
        assert!(run_on(r#"const re = /\d+/;"#).is_empty());
    }


    #[test]
    fn allows_valid_escape_w() {
        assert!(run_on(r#"const re = /\w+/;"#).is_empty());
    }


    #[test]
    fn allows_escaped_dot() {
        assert!(run_on(r#"const re = /\./;"#).is_empty());
    }


    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "bg-[url(\a)]";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_backslash_in_url_string() {
        let src = r#"const u = "http://a/\foo";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_empty_scoped_import() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }
}
