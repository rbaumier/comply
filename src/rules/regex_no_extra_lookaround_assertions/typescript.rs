//! regex-no-extra-lookaround-assertions TypeScript / JavaScript / TSX
//! backend.
//!
//! Flags useless nested lookaround assertions that can be inlined,
//! e.g. `(?=(?=a))`. AST-only detection eliminates FPs from strings.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

fn has_extra_lookaround(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 4 < len {
        if bytes[i] == b'(' && bytes[i + 1] == b'?' {
            let is_lookaround = matches!(bytes[i + 2], b'=' | b'!')
                || (bytes[i + 2] == b'<' && i + 3 < len && matches!(bytes[i + 3], b'=' | b'!'));

            if is_lookaround {
                let content_start = if bytes[i + 2] == b'<' { i + 4 } else { i + 3 };
                if content_start < len {
                    let trimmed = &pattern[content_start..];
                    if trimmed.starts_with("(?=")
                        || trimmed.starts_with("(?!")
                        || trimmed.starts_with("(?<=")
                        || trimmed.starts_with("(?<!")
                    {
                        if let Some(inner_close) = find_matching_paren(bytes, content_start)
                            && inner_close + 1 < len
                            && bytes[inner_close + 1] == b')'
                        {
                            return true;
                        }
                    }
                }
            }
        }
        i += 1;
    }
    false
}

fn find_matching_paren(bytes: &[u8], start: usize) -> Option<usize> {
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

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_extra_lookaround(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-extra-lookaround-assertions",
        "Useless nested lookaround assertion \u{2014} it can be inlined.".into(),
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
    fn flags_nested_lookahead() {
        assert_eq!(run_on(r#"const re = /(?=(?=a))/;"#).len(), 1);
    }

    #[test]
    fn allows_single_lookahead() {
        assert!(run_on(r#"const re = /(?=a)/;"#).is_empty());
    }

    #[test]
    fn flags_nested_negative_lookahead() {
        assert_eq!(run_on(r#"const re = /(?!(?!a))/;"#).len(), 1);
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
