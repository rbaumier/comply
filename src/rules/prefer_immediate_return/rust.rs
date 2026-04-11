//! prefer-immediate-return Rust backend.
//!
//! Flag `let x = expr; return x;` or `let x = expr; x` (tail expression)
//! that should be simplified to `return expr;` or just `expr`.

use crate::diagnostic::{Diagnostic, Severity};

fn extract_assigned_name(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("let mut ")
        .or_else(|| line.strip_prefix("let "))?;
    let eq_pos = rest.find('=')?;
    let name = rest[..eq_pos].trim();
    if name.is_empty()
        || name.contains('{')
        || name.contains('(')
        || name.contains(':')
        || name.contains(' ')
    {
        return None;
    }
    Some(name)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "source_file" {
        return;
    }
    let src = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = src.lines().collect();

    let non_blank: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, _)| i)
        .collect();

    for pair in non_blank.windows(2) {
        let (i, j) = (pair[0], pair[1]);
        let assign_line = lines[i].trim();
        let return_line = lines[j].trim();

        if assign_line.starts_with("//") || assign_line.starts_with("///") {
            continue;
        }

        let name = extract_assigned_name(assign_line);
        if let Some(name) = name {
            // `return name;`
            let expected_return = format!("return {};", name);
            let expected_return_no_semi = format!("return {name}");
            // Tail expression: just `name` (possibly with semicolon stripped)
            let is_tail = return_line == name || return_line == format!("{name};");

            if return_line == expected_return || return_line == expected_return_no_semi || is_tail {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: "prefer-immediate-return".into(),
                    message: format!(
                        "Variable `{name}` is assigned and immediately returned \u{2014} return the expression directly."
                    ),
                    severity: Severity::Warning,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_assign_then_return() {
        let src = "let result = compute_value();\nreturn result;";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 1);
    }

    #[test]
    fn flags_assign_then_tail_expr() {
        let src = "let result = compute_value();\nresult";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_assign_used_later() {
        let src = "let result = compute_value();\nprintln!(\"{}\", result);\nresult";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_different_variable_returned() {
        assert!(run_on("let result = compute_value();\nreturn other;").is_empty());
    }
}
