//! no-redundant-assignment backend — variable assigned then immediately overwritten.

use crate::diagnostic::{Diagnostic, Severity};

/// Try to extract the variable name from an assignment statement.
/// Handles:
///   `let x = ...;`  `const x = ...;`  `var x = ...;`  `x = ...;`
/// Returns the variable name if found.
fn extract_assignment_target(line: &str) -> Option<&str> {
    let trimmed = line.trim();

    // Skip comments, control flow, return statements
    if trimmed.starts_with("//")
        || trimmed.starts_with('*')
        || trimmed.starts_with("/*")
        || trimmed.starts_with("if ")
        || trimmed.starts_with("if(")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("for(")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("return ")
        || trimmed.starts_with("else")
        || trimmed.starts_with('{')
        || trimmed.starts_with('}')
    {
        return None;
    }

    // Strip let/const/var prefix
    let rest = if let Some(r) = trimmed.strip_prefix("let ") {
        r.trim_start()
    } else if let Some(r) = trimmed.strip_prefix("const ") {
        r.trim_start()
    } else if let Some(r) = trimmed.strip_prefix("var ") {
        r.trim_start()
    } else {
        trimmed
    };

    // Find `=` (but not `==` or `===` or `=>`)
    let eq_pos = rest.find('=')?;
    if eq_pos == 0 {
        return None;
    }
    // Check the character after `=` — must not be `=` or `>`
    let after = rest.as_bytes().get(eq_pos + 1)?;
    if *after == b'=' || *after == b'>' {
        return None;
    }

    let candidate = rest[..eq_pos].trim();

    // Must be a simple identifier (with optional type annotation)
    // Strip type annotation: `x: number` -> `x`
    let name = if let Some(colon) = candidate.find(':') {
        candidate[..colon].trim()
    } else {
        candidate
    };

    if name.is_empty() {
        return None;
    }

    // Validate that name is a simple identifier (alphanumeric + underscore + optional dots for member access)
    if name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    {
        Some(name)
    } else {
        None
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = text.lines().collect();

    for i in 0..lines.len().saturating_sub(1) {
        let current = extract_assignment_target(lines[i]);
        let next = extract_assignment_target(lines[i + 1]);

        if let (Some(curr_var), Some(next_var)) = (current, next)
            && curr_var == next_var {
                // Don't flag `const` — reassigning a const would be a syntax error,
                // so the second line is actually a different scope or destructuring.
                let curr_trimmed = lines[i].trim();
                if curr_trimmed.starts_with("const ") {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: "no-redundant-assignment".into(),
                    message: format!(
                        "Variable `{}` is assigned on line {} then immediately overwritten on line {}.",
                        curr_var,
                        i + 1,
                        i + 2,
                    ),
                    severity: Severity::Error,
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
    fn flags_immediate_overwrite() {
        let d = run_on("let x = 1;\nx = 2;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn flags_reassignment_pair() {
        let d = run_on("x = foo();\nx = bar();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_different_variables() {
        assert!(run_on("let x = 1;\nlet y = 2;").is_empty());
    }

    #[test]
    fn allows_used_between() {
        assert!(run_on("let x = 1;\nconsole.log(x);\nx = 2;").is_empty());
    }
}
