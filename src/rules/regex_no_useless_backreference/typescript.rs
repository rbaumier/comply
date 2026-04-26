//! regex-no-useless-backreference TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! URLs, Tailwind arbitrary-value classes, and scoped import paths
//! inside string literals cannot false-positive as regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Returns `true` when `pattern` contains a backreference that always
/// resolves to the empty string — i.e. a self-reference (`(\1)`) or a
/// forward-reference (`\1(a)`).
fn has_useless_backref(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut group_count = 0usize;
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            if bytes[i + 1].is_ascii_digit() && bytes[i + 1] != b'0' {
                let ref_num = (bytes[i + 1] - b'0') as usize;
                // Forward reference: group hasn't been opened yet.
                if ref_num > group_count {
                    return true;
                }
            }
            i += 2;
            continue;
        }
        if bytes[i] == b'(' && (i + 1 >= bytes.len() || bytes[i + 1] != b'?') {
            group_count += 1;
            // Self-reference: `(\N)` where N == current group_count.
            let inner_start = i + 1;
            if inner_start + 1 < bytes.len()
                && bytes[inner_start] == b'\\'
                && bytes[inner_start + 1].is_ascii_digit()
            {
                let ref_num = (bytes[inner_start + 1] - b'0') as usize;
                if ref_num == group_count {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_useless_backref(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-useless-backreference",
        "Backreference always resolves to the empty string \u{2014} it references itself or a forward group.".into(),
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
    fn flags_forward_backreference() {
        assert_eq!(run_on(r#"const re = /\1(a)/;"#).len(), 1);
    }

    #[test]
    fn flags_self_reference() {
        assert_eq!(run_on(r#"const re = /(\1)/;"#).len(), 1);
    }

    #[test]
    fn allows_valid_backreference() {
        assert!(run_on(r#"const re = /(a)\1/;"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/b";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_with_empty_segment() {
        let src = r#"import X from "@scope//pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
