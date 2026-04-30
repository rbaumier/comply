//! regex-confusing-quantifier TypeScript / JavaScript / TSX backend.
//!
//! Flags patterns where a group whose content can match empty (because
//! its elements are all optional with `?` / `*`) is wrapped in a
//! required quantifier (`+` or `{n,}` with `n>0`). The quantifier's
//! minimum is non-zero but the match-empty inner means the whole
//! pattern can still consume nothing — usually a bug.
//!
//! AST-only detection eliminates the TextCheck false-positive class
//! where string literals containing `(?:...)`-shaped text were parsed
//! as regex.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

fn has_confusing_quantifier(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'(' {
            let mut depth = 1;
            let mut j = i + 1;
            let mut inner_has_optional = false;

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
                    b'?' if depth == 1
                        && j > 0
                        && bytes[j - 1] != b'('
                        && bytes[j - 1] != b'\\' =>
                    {
                        inner_has_optional = true;
                    }
                    b'*' if depth == 1 => {
                        inner_has_optional = true;
                    }
                    _ => {}
                }
                j += 1;
            }

            if depth == 0 && inner_has_optional && j + 1 < len {
                let next = bytes[j + 1];
                if next == b'+' {
                    return true;
                } else if next == b'{'
                    && let Some(min) = parse_min_quantifier(&pattern[j + 1..])
                    && min > 0
                {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

fn parse_min_quantifier(s: &str) -> Option<usize> {
    if !s.starts_with('{') {
        return None;
    }
    let inner = &s[1..];
    let end = inner.find('}')?;
    let content = &inner[..end];
    let parts: Vec<&str> = content.split(',').collect();
    parts.first()?.parse().ok()
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_confusing_quantifier(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-confusing-quantifier",
        "Confusing quantifier \u{2014} minimum is non-zero but the element can match empty string.".into(),
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
    fn flags_optional_in_plus_group() {
        assert_eq!(run_on(r#"const re = /(?:a?)+/;"#).len(), 1);
    }

    #[test]
    fn allows_required_in_plus_group() {
        assert!(run_on(r#"const re = /(?:a)+/;"#).is_empty());
    }

    #[test]
    fn flags_star_in_plus_group() {
        assert_eq!(run_on(r#"const re = /(?:a*)+/;"#).len(), 1);
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
