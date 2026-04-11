//! no-useless-intersection AST backend — intersection with `any` or `unknown`.

use crate::diagnostic::{Diagnostic, Severity};

/// Detect `& any`, `& unknown`, `any &`, `unknown &` in type expressions.
fn has_useless_intersection(line: &str) -> bool {
    let trimmed = line.trim();
    for pattern in &["& any", "& unknown", "any &", "unknown &"] {
        if let Some(pos) = trimmed.find(pattern) {
            let before = if pos > 0 {
                trimmed.as_bytes()[pos - 1]
            } else {
                b' '
            };
            let end = pos + pattern.len();
            let after = if end < trimmed.len() {
                trimmed.as_bytes()[end]
            } else {
                b' '
            };
            if !before.is_ascii_alphanumeric() && before != b'_'
                && !after.is_ascii_alphanumeric() && after != b'_'
            {
                return true;
            }
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in text.lines().enumerate() {
        if has_useless_intersection(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-useless-intersection".into(),
                message: "Intersection with `any` or `unknown` is useless — remove it.".into(),
                severity: Severity::Warning,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_intersection_with_any() {
        assert_eq!(run_on("type X = Foo & any;").len(), 1);
    }

    #[test]
    fn flags_intersection_with_unknown() {
        assert_eq!(run_on("type X = Foo & unknown;").len(), 1);
    }

    #[test]
    fn flags_any_on_left() {
        assert_eq!(run_on("type X = any & Foo;").len(), 1);
    }

    #[test]
    fn allows_normal_intersection() {
        assert!(run_on("type X = Foo & Bar;").is_empty());
    }

    #[test]
    fn no_false_positive_on_any_prefix() {
        assert!(run_on("type X = anything & Foo;").is_empty());
    }
}
