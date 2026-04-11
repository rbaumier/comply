//! prefer-date-now backend — flag `new Date().getTime()`, `.valueOf()`, `+new Date()`, `Number(new Date())`.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a node is `new Date()` (no arguments).
fn is_new_date_no_args(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "new_expression" {
        return false;
    }
    let Some(constructor) = node.child_by_field_name("constructor") else { return false };
    if constructor.utf8_text(source).unwrap_or("") != "Date" {
        return false;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return false };
    // arguments node should have no named children (empty parens)
    args.named_child_count() == 0
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Pattern 1: `new Date().getTime()` / `new Date().valueOf()`
    if node.kind() == "call_expression" {
        let Some(func) = node.child_by_field_name("function") else { return };
        if func.kind() == "member_expression" {
            let Some(prop) = func.child_by_field_name("property") else { return };
            let prop_name = prop.utf8_text(source).unwrap_or("");
            if prop_name == "getTime" || prop_name == "valueOf" {
                let Some(obj) = func.child_by_field_name("object") else { return };
                if is_new_date_no_args(obj, source) {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "prefer-date-now".into(),
                        message: "Prefer `Date.now()` over `new Date().getTime()`/`.valueOf()`.".into(),
                        severity: Severity::Warning,
                    });
                    return;
                }
            }
            // Pattern 3: `Number(new Date())`
            if func.kind() == "member_expression" {
                return;
            }
        }

        // Pattern 3: `Number(new Date())`
        if func.kind() == "identifier" && func.utf8_text(source).unwrap_or("") == "Number" {
            let Some(args) = node.child_by_field_name("arguments") else { return };
            if args.named_child_count() == 1 {
                let arg = args.named_child(0).unwrap();
                if is_new_date_no_args(arg, source) {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "prefer-date-now".into(),
                        message: "Prefer `Date.now()` over `Number(new Date())`.".into(),
                        severity: Severity::Warning,
                    });
                    return;
                }
            }
        }
    }

    // Pattern 2: `+new Date()` — unary_expression with operator `+`
    if node.kind() == "unary_expression" {
        let op = node.child_by_field_name("operator")
            .and_then(|o| o.utf8_text(source).ok())
            .unwrap_or("");
        if op == "+" {
            let Some(arg) = node.child_by_field_name("argument") else { return };
            if is_new_date_no_args(arg, source) {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "prefer-date-now".into(),
                    message: "Prefer `Date.now()` over `+new Date()`.".into(),
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
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_get_time() {
        let d = run_on("const ts = new Date().getTime();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-date-now");
    }

    #[test]
    fn flags_value_of() {
        let d = run_on("const ts = new Date().valueOf();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_unary_plus() {
        let d = run_on("const ts = +new Date();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_number_coercion() {
        let d = run_on("const ts = Number(new Date());");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_date_now() {
        assert!(run_on("const ts = Date.now();").is_empty());
    }

    #[test]
    fn allows_new_date_with_args() {
        assert!(run_on("const d = new Date(2024, 0, 1).getTime();").is_empty());
    }
}
