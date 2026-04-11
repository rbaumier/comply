use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `.removeEventListener(` followed by an inline listener that will
/// never match: arrow/function expressions or `.bind(` calls.
fn is_invalid_remove_listener(line: &str) -> bool {
    let Some(pos) = line.find(".removeEventListener(") else {
        return false;
    };
    let after = &line[pos + ".removeEventListener(".len()..];

    // Skip past the first argument (event name) by finding the first comma
    // that is not inside a string literal.
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

    // Case 2: `.bind(` call — the second argument contains `.bind(`
    if listener_part.contains(".bind(") {
        return true;
    }

    false
}

/// Find the first comma at parenthesis depth 0 (handles nested parens in the
/// event-name argument, which is rare but possible).
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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_invalid_remove_listener(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-invalid-remove-event-listener".into(),
                    message: "The listener argument should be a function reference — inline functions and `.bind()` create a new reference each call."
                        .into(),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_bind_call() {
        let code = r#"el.removeEventListener('click', handler.bind(this));"#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_arrow_function() {
        let code = r#"el.removeEventListener('click', () => handler());"#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_function_expression() {
        let code = r#"el.removeEventListener('click', function() { handler(); });"#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_function_reference() {
        let code = r#"el.removeEventListener('click', handler);"#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_variable_reference() {
        let code = r#"el.removeEventListener('click', this.onClickBound);"#;
        assert!(run(code).is_empty());
    }
}
