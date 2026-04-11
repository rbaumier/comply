//! prefer-string-starts-ends-with backend — flag `/^simple/.test()` and `/simple$/.test()`.

use crate::diagnostic::{Diagnostic, Severity};

/// Characters that make a regex pattern "complex" (not a simple literal string).
const SPECIAL_CHARS: &[char] = &['^', '$', '+', '[', '{', '(', '\\', '.', '?', '*', '|'];

/// Returns true if `s` contains none of the special regex characters.
fn is_simple_string(s: &str) -> bool {
    !s.chars().any(|c| SPECIAL_CHARS.contains(&c))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }

    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "test" {
        return;
    }

    let Some(obj) = func.child_by_field_name("object") else { return };
    if obj.kind() != "regex" {
        return;
    }

    // Check flags — skip if `i` or `m` present
    if let Some(flags_node) = obj.child_by_field_name("flags") {
        let flags = flags_node.utf8_text(source).unwrap_or("");
        if flags.contains('i') || flags.contains('m') {
            return;
        }
    }

    // Get the regex pattern
    let Some(pattern_node) = obj.child_by_field_name("pattern") else { return };
    let pattern = pattern_node.utf8_text(source).unwrap_or("");

    if pattern.is_empty() {
        return;
    }

    // Check for ^prefix pattern
    if let Some(literal) = pattern.strip_prefix('^')
        && is_simple_string(literal) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-string-starts-ends-with".into(),
                message: "Prefer `String#startsWith()` over a regex with `^`.".into(),
                severity: Severity::Warning,
            });
            return;
        }

    // Check for suffix$ pattern
    if let Some(literal) = pattern.strip_suffix('$')
        && is_simple_string(literal) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-string-starts-ends-with".into(),
                message: "Prefer `String#endsWith()` over a regex with `$`.".into(),
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
    fn flags_starts_with_regex() {
        let d = run_on(r#"/^foo/.test(str)"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("startsWith"));
    }

    #[test]
    fn flags_ends_with_regex() {
        let d = run_on(r#"/bar$/.test(str)"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("endsWith"));
    }

    #[test]
    fn ignores_case_insensitive() {
        assert!(run_on(r#"/^foo/i.test(str)"#).is_empty());
    }

    #[test]
    fn ignores_multiline() {
        assert!(run_on(r#"/^foo/m.test(str)"#).is_empty());
    }

    #[test]
    fn allows_complex_regex() {
        assert!(run_on(r#"/^fo+o/.test(str)"#).is_empty());
    }

    #[test]
    fn allows_non_test_call() {
        assert!(run_on(r#"/^foo/.exec(str)"#).is_empty());
    }
}
