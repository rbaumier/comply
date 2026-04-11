//! no-loop-counter-reassign AST backend — flag reassignment of `for` loop
//! counter inside the loop body.

use crate::diagnostic::{Diagnostic, Severity};

/// Extract the loop variable name from a `for (let/var/const IDENT ...` header.
fn extract_for_var(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with("for") {
        return None;
    }
    let rest = trimmed.strip_prefix("for")?.trim_start();
    let rest = rest.strip_prefix('(')?.trim_start();
    let rest = rest
        .strip_prefix("let")
        .or_else(|| rest.strip_prefix("var"))
        .or_else(|| rest.strip_prefix("const"))?
        .trim_start();
    let end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    Some(&rest[..end])
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        if let Some(var_name) = extract_for_var(lines[i]) {
            let mut depth: i32 = 0;
            let mut body_started = false;
            for ch in lines[i].chars() {
                if ch == '{' {
                    depth += 1;
                    body_started = true;
                } else if ch == '}' {
                    depth -= 1;
                }
            }
            let mut j = i + 1;
            while j < lines.len() {
                let line = lines[j];
                for ch in line.chars() {
                    if ch == '{' {
                        depth += 1;
                        body_started = true;
                    } else if ch == '}' {
                        depth -= 1;
                    }
                }
                if body_started && depth <= 0 {
                    break;
                }
                let trimmed = line.trim();
                if let Some(pos) = trimmed.find(var_name) {
                    let after_var = &trimmed[pos + var_name.len()..].trim_start();
                    if after_var.starts_with('=') && !after_var.starts_with("==") {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: j + 1,
                            column: 1,
                            rule_id: "no-loop-counter-reassign".into(),
                            message: format!(
                                "Loop counter `{var_name}` is reassigned inside the loop body."
                            ),
                            severity: Severity::Error,
                        });
                    }
                }
                j += 1;
            }
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_counter_reassign() {
        let src = "for (let i = 0; i < n; i++) {\n  i = 5;\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_counter_reassign_var() {
        let src = "for (var j = 0; j < 10; j++) {\n  j = 0;\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_normal_loop() {
        let src = "for (let i = 0; i < n; i++) {\n  console.log(i);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_different_var() {
        let src = "for (let i = 0; i < n; i++) {\n  x = 5;\n}";
        assert!(run_on(src).is_empty());
    }
}
