//! regex-use-unicode-flag TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! URLs, Tailwind arbitrary-value classes, and import paths inside
//! string literals cannot false-positive as regex literals with
//! `\p{...}` escapes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Returns true if the regex pattern contains a `\p{...}` or `\P{...}`
/// Unicode property escape (respecting backslash escaping).
fn has_unicode_property_escape(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if (next == b'p' || next == b'P') && i + 2 < bytes.len() && bytes[i + 2] == b'{' {
                return true;
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    false
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, flags)) = pattern_and_flags(&node, source) else { return };
    if !has_unicode_property_escape(pattern) {
        return;
    }
    if flags.contains('u') || flags.contains('v') {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-use-unicode-flag",
        "Unicode property escape (`\\p{...}`) requires the `u` or `v` flag.".into(),
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
    fn flags_unicode_prop_without_u() {
        let diags = run_on(r#"const re = /\p{Letter}/;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_uppercase_p_without_u() {
        let diags = run_on(r#"const re = /\P{Number}/i;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_unicode_prop_with_u() {
        assert!(run_on(r#"const re = /\p{Letter}/u;"#).is_empty());
    }

    #[test]
    fn allows_unicode_prop_with_v() {
        assert!(run_on(r#"const re = /\p{Letter}/v;"#).is_empty());
    }

    #[test]
    fn allows_regex_without_unicode_escape() {
        assert!(run_on(r#"const re = /abc/;"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_in_string() {
        let src = r#"const x = "has-[\p{foo}]:grid";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://example.com/\\p{Letter}/path";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_with_empty_flags() {
        let src = r#"import X from "@scope/pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
