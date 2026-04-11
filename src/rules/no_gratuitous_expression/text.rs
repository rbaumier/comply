use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Patterns that indicate a gratuitous (always-true or always-false) expression.
const ALWAYS_TRUE_FALSE: &[(&str, &str)] = &[
    ("if (true)", "condition is always true"),
    ("if (false)", "condition is always false"),
    ("if(true)", "condition is always true"),
    ("if(false)", "condition is always false"),
    ("&& false)", "expression is always false (short-circuited by `&& false`)"),
    ("&& false,", "expression is always false (short-circuited by `&& false`)"),
    ("&& false;", "expression is always false (short-circuited by `&& false`)"),
    ("|| true)", "expression is always true (short-circuited by `|| true`)"),
    ("|| true,", "expression is always true (short-circuited by `|| true`)"),
    ("|| true;", "expression is always true (short-circuited by `|| true`)"),
];

fn detect_self_comparison(line: &str) -> Option<&str> {
    // Detect `x === x` or `x == x` or `x !== x` or `x != x`
    for op in &["===", "!==", "==", "!="] {
        if let Some(pos) = line.find(op) {
            let before = line[..pos].trim();
            let after = line[pos + op.len()..].trim();
            // Extract the last token before the operator
            let lhs = before.rsplit(|c: char| c == '(' || c == ' ' || c == '!').next().unwrap_or("").trim();
            // Extract the first token after the operator
            let rhs = after.split(|c: char| c == ')' || c == ' ' || c == ';' || c == ',').next().unwrap_or("").trim();
            if !lhs.is_empty()
                && lhs == rhs
                && lhs.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.')
            {
                if op.starts_with('!') {
                    return Some("comparison `x !== x` is always false (unless NaN)");
                }
                return Some("comparison `x === x` is always true (unless NaN)");
            }
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();

            // Skip comments
            if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
                continue;
            }

            // Check constant boolean patterns
            for (pattern, message) in ALWAYS_TRUE_FALSE {
                if trimmed.contains(pattern) {
                    // Exception: `while (true)` with a `break` is an intentional
                    // infinite-loop idiom — skip it.
                    if (trimmed.starts_with("while (true)") || trimmed.starts_with("while(true)"))
                        && ctx.source.contains("break")
                    {
                        continue;
                    }

                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-gratuitous-expression".into(),
                        message: format!(
                            "Gratuitous expression: {}.",
                            message
                        ),
                        severity: Severity::Error,
                    });
                }
            }

            // Check self-comparison
            if let Some(message) = detect_self_comparison(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: format!("Gratuitous expression: {}.", message),
                    severity: Severity::Error,
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
    fn flags_if_true() {
        let d = run("if (true) { doStuff(); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always true"));
    }

    #[test]
    fn flags_if_false() {
        let d = run("if (false) { doStuff(); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always false"));
    }

    #[test]
    fn flags_and_false() {
        let d = run("if (x && false) { doStuff(); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always false"));
    }

    #[test]
    fn flags_or_true() {
        let d = run("const val = (x || true);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always true"));
    }

    #[test]
    fn flags_self_comparison() {
        let d = run("if (x === x) { doStuff(); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always true"));
    }

    #[test]
    fn allows_normal_conditions() {
        assert!(run("if (x > 0) { doStuff(); }").is_empty());
    }

    #[test]
    fn allows_while_true_with_break() {
        let source = r#"
while (true) {
    if (done) break;
    process();
}
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_different_comparison() {
        assert!(run("if (x === y) { doStuff(); }").is_empty());
    }
}
