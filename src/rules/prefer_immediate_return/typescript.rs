use crate::diagnostic::{Diagnostic, Severity};

/// Extract the identifier name from a `const/let/var name = ...;` line.
fn extract_assigned_name(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("const ")
        .or_else(|| line.strip_prefix("let "))
        .or_else(|| line.strip_prefix("var "))?;
    let eq_pos = rest.find('=')?;
    let name = rest[..eq_pos].trim();
    if name.is_empty()
        || name.contains('{')
        || name.contains('[')
        || name.contains(':')
        || name.contains(' ')
    {
        return None;
    }
    Some(name)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }
    let src = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = src.lines().collect();

    // Collect non-blank line indices for pairwise scanning.
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

        if assign_line.starts_with("//") || assign_line.starts_with('*') {
            continue;
        }

        let name = extract_assigned_name(assign_line);
        if let Some(name) = name {
            let expected = format!("return {};", name);
            if return_line == expected || return_line == format!("return {name}") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: "prefer-immediate-return".into(),
                    message: format!(
                        "Variable `{name}` is assigned and immediately returned — return the expression directly."
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

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_assign_then_return() {
        let src = "const result = computeValue();\nreturn result;";
        let d = run_ts(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 1);
    }

    #[test]
    fn flags_let_assign_then_return() {
        let src = "let x = a + b;\nreturn x;";
        assert_eq!(run_ts(src).len(), 1);
    }

    #[test]
    fn allows_assign_used_later() {
        let src = "const result = computeValue();\nconsole.log(result);\nreturn result;";
        assert!(run_ts(src).is_empty());
    }

    #[test]
    fn allows_different_variable_returned() {
        assert!(run_ts("const result = computeValue();\nreturn other;").is_empty());
    }

    #[test]
    fn allows_destructuring() {
        assert!(run_ts("const { a, b } = getValues();\nreturn a;").is_empty());
    }

    #[test]
    fn flags_with_blank_line_between() {
        let src = "const result = computeValue();\n\nreturn result;";
        assert_eq!(run_ts(src).len(), 1);
    }
}
