//! no-invalid-remove-event-listener AST backend — flag `removeEventListener`
//! with inline functions or `.bind()` calls.

use crate::diagnostic::{Diagnostic, Severity};

/// Detect `.removeEventListener(` followed by an inline listener that will
/// never match: arrow/function expressions or `.bind(` calls.
fn is_invalid_remove_listener(line: &str) -> bool {
    let Some(pos) = line.find(".removeEventListener(") else {
        return false;
    };
    let after = &line[pos + ".removeEventListener(".len()..];

    let Some(comma) = find_top_level_comma(after) else {
        return false;
    };
    let listener_part = after[comma + 1..].trim_start();

    // Case 1: inline arrow or function expression
    if listener_part.starts_with("(")
        || listener_part.starts_with("function")
        || listener_part.starts_with("function(")
    {
        return true;
    }

    // Case 2: `.bind(` call
    if listener_part.contains(".bind(") {
        return true;
    }

    false
}

/// Find the first comma at parenthesis depth 0.
fn find_top_level_comma(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut prev = '\0';

    for (i, ch) in s.char_indices() {
        match ch {
            '\'' if !in_double && !in_backtick && prev != '\\' => in_single = !in_single,
            '"' if !in_single && !in_backtick && prev != '\\' => in_double = !in_double,
            '`' if !in_single && !in_double && prev != '\\' => in_backtick = !in_backtick,
            '(' if !in_single && !in_double && !in_backtick => depth += 1,
            ')' if !in_single && !in_double && !in_backtick => depth -= 1,
            ',' if !in_single && !in_double && !in_backtick && depth == 0 => return Some(i),
            _ => {}
        }
        prev = ch;
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in text.lines().enumerate() {
        if is_invalid_remove_listener(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-invalid-remove-event-listener".into(),
                message: "The listener argument should be a function reference — inline functions and `.bind()` create a new reference each call."
                    .into(),
                severity: Severity::Warning,
                span: None,
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
    fn flags_bind_call() {
        let code = r#"el.removeEventListener('click', handler.bind(this));"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn flags_arrow_function() {
        let code = r#"el.removeEventListener('click', () => handler());"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn flags_function_expression() {
        let code = r#"el.removeEventListener('click', function() { handler(); });"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn allows_function_reference() {
        let code = r#"el.removeEventListener('click', handler);"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_variable_reference() {
        let code = r#"el.removeEventListener('click', this.onClickBound);"#;
        assert!(run_on(code).is_empty());
    }
}
