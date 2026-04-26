//! prefer-less-than Rust backend — flag `binary_expression` nodes using `>`
//! or `>=` and suggest the equivalent `<` / `<=` form with operands swapped.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let op = op_node.utf8_text(source).unwrap_or("");

    let suggested = match op {
        ">" => "<",
        ">=" => "<=",
        _ => return,
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-less-than".into(),
        message: format!(
            "Prefer `{suggested}` over `{op}` for readability \u{2014} swap operands and use `{suggested}`."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_greater_than() {
        let d = run_on("fn f(a: i32, b: i32) -> bool { b > a }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<`"));
    }

    #[test]
    fn flags_greater_or_equal() {
        let d = run_on("fn f(a: i32, b: i32) -> bool { b >= a }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<=`"));
    }

    #[test]
    fn flags_inside_if() {
        assert_eq!(
            run_on("fn f(x: i32) { if x > 0 { g(); } }").len(),
            1
        );
    }

    #[test]
    fn allows_less_than() {
        assert!(run_on("fn f(a: i32, b: i32) -> bool { a < b }").is_empty());
    }

    #[test]
    fn allows_less_or_equal() {
        assert!(run_on("fn f(a: i32, b: i32) -> bool { a <= b }").is_empty());
    }

    #[test]
    fn allows_equality() {
        assert!(run_on("fn f(a: i32, b: i32) -> bool { a == b }").is_empty());
    }
}
