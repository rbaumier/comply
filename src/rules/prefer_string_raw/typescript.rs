//! prefer-string-raw backend — flag string literals with multiple escaped backslashes.

use crate::diagnostic::{Diagnostic, Severity};

/// Count `\\` pairs in a string node's source text.
fn count_escaped_backslashes(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut count = 0;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\\' && bytes[i + 1] == b'\\' {
            count += 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    count
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Only inspect string nodes (single/double quoted string literals).
    // Skip template_string since those already support String.raw.
    if node.kind() != "string" {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");

    // Already using String.raw — check parent
    if let Some(parent) = node.parent()
        && parent.kind() == "call_expression" {
            let func_text = parent
                .child_by_field_name("function")
                .and_then(|f| f.utf8_text(source).ok())
                .unwrap_or("");
            if func_text.contains("String.raw") {
                return;
            }
        }

    // Skip strings containing backticks (can't use String.raw with backticks)
    if text.contains('`') {
        return;
    }

    // Skip strings with interpolation patterns
    if text.contains("${") {
        return;
    }

    if count_escaped_backslashes(text) >= 2 {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "prefer-string-raw".into(),
            message: "`String.raw` should be used to avoid escaping `\\`.".into(),
            severity: Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_multiple_escaped_backslashes_double_quotes() {
        let d = run_on(r#"const p = "C:\\Users\\foo\\bar";"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-string-raw");
    }

    #[test]
    fn flags_multiple_escaped_backslashes_single_quotes() {
        let d = run_on(r#"const p = 'C:\\Users\\foo';"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_single_escaped_backslash() {
        assert!(run_on(r#"const p = "foo\\bar";"#).is_empty());
    }

    #[test]
    fn allows_no_backslash() {
        assert!(run_on(r#"const p = "hello world";"#).is_empty());
    }
}
