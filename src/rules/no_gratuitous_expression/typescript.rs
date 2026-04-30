//! no-gratuitous-expression backend — flag boolean expressions that are
//! always true or always false.

use crate::diagnostic::{Diagnostic, Severity};

fn detect_self_comparison(text: &str) -> Option<&'static str> {
    for op in &["===", "!==", "==", "!="] {
        if let Some(pos) = text.find(op) {
            let before = text[..pos].trim();
            let after = text[pos + op.len()..].trim();
            let lhs = before.rsplit(['(', ' ', '!']).next().unwrap_or("").trim();
            let rhs = after
                .split([')', ' ', ';', ','])
                .next()
                .unwrap_or("")
                .trim();
            if !lhs.is_empty()
                && lhs == rhs
                && lhs
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
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

crate::ast_check! { on ["if_statement", "binary_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        "if_statement" => {
            let Some(condition) = node.child_by_field_name("condition") else { return };
            let Ok(cond_text) = condition.utf8_text(source) else { return };
            let inner = cond_text.trim()
                .strip_prefix('(').unwrap_or(cond_text.trim())
                .strip_suffix(')').unwrap_or(cond_text.trim());
            if inner == "true" {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: "Gratuitous expression: condition is always true.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            } else if inner == "false" {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: "Gratuitous expression: condition is always false.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        "binary_expression" => {
            let Ok(text) = node.utf8_text(source) else { return };
            // Check `&& false` / `|| true`
            if text.ends_with("&& false") || text.contains("&& false)") || text.contains("&& false,") || text.contains("&& false;") {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: "Gratuitous expression: expression is always false (short-circuited by `&& false`).".into(),
                    severity: Severity::Error,
                    span: None,
                });
                return;
            }
            if text.ends_with("|| true") || text.contains("|| true)") || text.contains("|| true,") || text.contains("|| true;") {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: "Gratuitous expression: expression is always true (short-circuited by `|| true`).".into(),
                    severity: Severity::Error,
                    span: None,
                });
                return;
            }
            // Check self-comparison
            if let Some(message) = detect_self_comparison(text) {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: format!("Gratuitous expression: {}.", message),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_if_true() {
        let d = run_on("if (true) { doStuff(); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always true"));
    }

    #[test]
    fn flags_if_false() {
        let d = run_on("if (false) { doStuff(); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always false"));
    }

    #[test]
    fn flags_self_comparison() {
        let d = run_on("if (x === x) { doStuff(); }");
        assert!(!d.is_empty());
        assert!(d.iter().any(|d| d.message.contains("always true")));
    }

    #[test]
    fn allows_normal_conditions() {
        assert!(run_on("if (x > 0) { doStuff(); }").is_empty());
    }

    #[test]
    fn allows_different_comparison() {
        assert!(run_on("if (x === y) { doStuff(); }").is_empty());
    }
}
