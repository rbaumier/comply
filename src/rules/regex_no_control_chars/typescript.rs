//! regex-no-control-chars TypeScript / JavaScript / TSX backend.
//!
//! Detects control character escapes `\x00`..`\x1f` inside the
//! tree-sitter `regex` node's pattern — likely unintended. AST gating
//! eliminates FPs from string literals that contain backslash-x byte
//! sequences but aren't regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

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
                // Single hex digit: \x0 through \xf — all are control.
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

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_control_chars(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-control-chars",
        "Control character escape (`\\x00`-`\\x1f`) in regex \u{2014} likely unintended.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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

    // --- Regression tests for the TextCheck false-positive class. ---

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
