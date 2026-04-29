//! prefer-less-than TS backend — flag `binary_expression` nodes using `>` or
//! `>=` and suggest the equivalent `<` / `<=` form with operands swapped.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let op = op_node.utf8_text(source).unwrap_or("");

    let suggested = match op {
        ">" => "<",
        ">=" => "<=",
        _ => return,
    };

    // Don't flag `x > 0`, `arr.length >= 1` etc. — variable-vs-literal
    // comparisons are universally written subject-first.
    if let Some(rhs) = node.child_by_field_name("right") {
        if matches!(rhs.kind(), "number" | "string" | "true" | "false" | "null"
                    | "undefined" | "unary_expression") {
            return;
        }
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-less-than".into(),
        message: format!(
            "Prefer `{suggested}` over `{op}` for readability — swap operands and use `{suggested}`."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_greater_than() {
        let d = run_on("const r = b > a;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<`"));
    }

    #[test]
    fn flags_greater_or_equal() {
        let d = run_on("const r = b >= a;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<=`"));
    }

    #[test]
    fn allows_variable_vs_literal() {
        assert!(run_on("if (x > 0) { f(); }").is_empty());
        assert!(run_on("if (arr.length >= 1) { f(); }").is_empty());
        assert!(run_on("const ok = count > 5;").is_empty());
    }

    #[test]
    fn allows_less_than() {
        assert!(run_on("const r = a < b;").is_empty());
    }

    #[test]
    fn allows_less_or_equal() {
        assert!(run_on("const r = a <= b;").is_empty());
    }

    #[test]
    fn allows_equality() {
        assert!(run_on("const r = a === b;").is_empty());
    }
}
