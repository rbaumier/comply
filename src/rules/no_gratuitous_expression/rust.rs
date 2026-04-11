//! no-gratuitous-expression Rust backend.
//!
//! Flag boolean expressions that are always true or always false.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    match node.kind() {
        "if_expression" => {
            let Some(condition) = node.child_by_field_name("condition") else { return };
            let Ok(cond_text) = condition.utf8_text(source) else { return };
            let inner = cond_text.trim();
            if inner == "true" {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: "Gratuitous expression: condition is always true.".into(),
                    severity: Severity::Error,
                });
            } else if inner == "false" {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: "Gratuitous expression: condition is always false.".into(),
                    severity: Severity::Error,
                });
            }
        }
        "binary_expression" => {
            let Ok(text) = node.utf8_text(source) else { return };
            // Check `&& false` / `|| true`
            if text.ends_with("&& false") || text.contains("&& false)") || text.contains("&& false;") {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: "Gratuitous expression: expression is always false (short-circuited by `&& false`).".into(),
                    severity: Severity::Error,
                });
                return;
            }
            if text.ends_with("|| true") || text.contains("|| true)") || text.contains("|| true;") {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: "Gratuitous expression: expression is always true (short-circuited by `|| true`).".into(),
                    severity: Severity::Error,
                });
                return;
            }
            // Check self-comparison: `x == x`, `x != x`
            if let Some(op_node) = node.child_by_field_name("operator")
                && let Ok(op) = op_node.utf8_text(source)
                && (op == "==" || op == "!=")
                && let Some(left) = node.child_by_field_name("left")
                && let Some(right) = node.child_by_field_name("right")
                && let Ok(lt) = left.utf8_text(source)
                && let Ok(rt) = right.utf8_text(source)
                && lt == rt
                && !lt.trim().is_empty()
            {
                let pos = node.start_position();
                let msg = if op == "!=" {
                    "Gratuitous expression: comparison `x != x` is always false."
                } else {
                    "Gratuitous expression: comparison `x == x` is always true."
                };
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: msg.into(),
                    severity: Severity::Error,
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
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_if_true() {
        let d = run_on("fn f() { if true { do_stuff(); } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always true"));
    }

    #[test]
    fn flags_if_false() {
        let d = run_on("fn f() { if false { do_stuff(); } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always false"));
    }

    #[test]
    fn allows_normal_conditions() {
        assert!(run_on("fn f(x: i32) { if x > 0 { do_stuff(); } }").is_empty());
    }
}
