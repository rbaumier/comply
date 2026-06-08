//! regex-no-control-chars OXC backend — flag control character escapes in regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn has_control_chars(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i] == b'\\' && bytes[i + 1] == b'x' {
            let h1 = bytes[i + 2];
            let h2 = bytes[i + 3];
            if h1.is_ascii_hexdigit() && h2.is_ascii_hexdigit() {
                let val = hex_val(h1) * 16 + hex_val(h2);
                if val <= 0x1f {
                    return true;
                }
            } else if h1.is_ascii_hexdigit() && !h2.is_ascii_hexdigit() {
                let val = hex_val(h1);
                if val <= 0x1f {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
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
        if !has_control_chars(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Control character escape (`\\x00`-`\\x1f`) in regex \u{2014} likely unintended."
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
    fn flags_null_byte() {
        assert_eq!(run_on(r#"const re = /\x00/;"#).len(), 1);
    }


    #[test]
    fn flags_control_char_1f() {
        assert_eq!(run_on(r#"const re = /\x1f/;"#).len(), 1);
    }


    #[test]
    fn allows_printable_hex() {
        assert!(run_on(r#"const re = /\x20/;"#).is_empty());
    }


    #[test]
    fn allows_upper_hex() {
        assert!(run_on(r#"const re = /\xFF/;"#).is_empty());
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
