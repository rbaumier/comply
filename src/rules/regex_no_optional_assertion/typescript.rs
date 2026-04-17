//! regex-no-optional-assertion TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! Tailwind arbitrary-value classes, URLs, and import paths inside
//! string literals cannot false-positive as regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

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
                let inner_start = i + 1;
                let mut k = inner_start;
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

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "regex" {
        return;
    }
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_optional_assertion(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-optional-assertion",
        "Assertion inside an optional group is effectively ignored.".into(),
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
    fn flags_assertion_in_optional_group() {
        assert_eq!(run_on(r#"const re = /(?:^foo)?bar/;"#).len(), 1);
    }

    #[test]
    fn allows_assertion_in_required_group() {
        assert!(run_on(r#"const re = /(?:^foo)bar/;"#).is_empty());
    }

    #[test]
    fn flags_assertion_in_star_group() {
        assert_eq!(run_on(r#"const re = /(?:^foo)*bar/;"#).len(), 1);
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        // Tailwind arbitrary-value classes contain `(` and `)?` sequences
        // that the old text scanner would flag.
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr] (^foo)?";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        // URLs with query params can produce `(^...)?`-looking substrings.
        let src = r#"const u = "https://example.com/x?y=(^a)?";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_empty_scoped_import_path() {
        let src = r#"import X from "";"#;
        assert!(run_on(src).is_empty());
    }
}
