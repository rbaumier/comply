//! no-redundant-boolean backend — redundant boolean literal in return or condition.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = text.lines().collect();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Pattern: `? true : false` or `? false : true` on a single line
        if (trimmed.contains("? true : false") || trimmed.contains("? false : true"))
            && !trimmed.starts_with("//")
            && !trimmed.starts_with('*')
        {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-redundant-boolean".into(),
                message: "Redundant ternary — simplify to the condition itself (or its negation).".into(),
                severity: Severity::Error,
            });
            continue;
        }

        // Pattern: `=== true` or `=== false` or `!== true` or `!== false`
        if (trimmed.contains("=== true") || trimmed.contains("=== false")
            || trimmed.contains("!== true") || trimmed.contains("!== false"))
            && !trimmed.starts_with("//")
            && !trimmed.starts_with('*')
        {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-redundant-boolean".into(),
                message: "Redundant boolean comparison — use the value directly.".into(),
                severity: Severity::Error,
            });
            continue;
        }

        // Pattern: `if (...) return true;` followed by `return false;` (or vice versa)
        if idx + 1 < lines.len() && trimmed.contains("if (") && !trimmed.starts_with("//") {
            let next = lines[idx + 1].trim();
            let has_return_true = trimmed.contains("return true");
            let has_return_false = trimmed.contains("return false");
            let next_return_true = next.contains("return true");
            let next_return_false = next.contains("return false");

            if (has_return_true && (next_return_false || next.contains("else") && next.contains("return false")))
                || (has_return_false && (next_return_true || next.contains("else") && next.contains("return true")))
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-redundant-boolean".into(),
                    message: "Redundant if/else returning boolean literals — return the condition directly.".into(),
                    severity: Severity::Error,
                });
                continue;
            }
        }

        // Pattern: multiline if/else blocks
        if idx + 3 < lines.len()
            && trimmed.contains("if (")
            && trimmed.ends_with('{')
            && !trimmed.starts_with("//")
        {
            let l1 = lines[idx + 1].trim();
            let l2 = lines[idx + 2].trim();
            let l3 = lines[idx + 3].trim();
            if (l1 == "return true;" && l2.starts_with("} else") && l3 == "return false;")
                || (l1 == "return false;" && l2.starts_with("} else") && l3 == "return true;")
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-redundant-boolean".into(),
                    message: "Redundant if/else returning boolean literals — return the condition directly.".into(),
                    severity: Severity::Error,
                });
            }
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
    fn flags_ternary_true_false() {
        assert_eq!(run_on("const x = cond ? true : false;").len(), 1);
    }

    #[test]
    fn flags_strict_equals_true() {
        assert_eq!(run_on("if (x === true) doSomething();").len(), 1);
    }

    #[test]
    fn flags_if_return_true_else_return_false() {
        assert_eq!(run_on("if (isValid) return true;\nreturn false;").len(), 1);
    }

    #[test]
    fn allows_normal_ternary() {
        assert!(run_on("const x = cond ? 'a' : 'b';").is_empty());
    }

    #[test]
    fn allows_comment_mentioning_true() {
        assert!(run_on("// returns true if valid").is_empty());
    }
}
