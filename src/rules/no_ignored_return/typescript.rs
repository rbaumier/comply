//! no-ignored-return backend — flag standalone calls to pure methods
//! whose return value is ignored.

use crate::diagnostic::{Diagnostic, Severity};

const PURE_METHODS: &[&str] = &[
    "map", "filter", "slice", "concat", "trim", "replace",
    "toUpperCase", "toLowerCase", "split", "join",
];

crate::ast_check! { on ["expression_statement"] => |node, source, ctx, diagnostics|
    // We only care about expression_statement nodes — a call as a
    // standalone statement means its return value is discarded.
    let Some(expr) = node.named_child(0) else { return };
    if expr.kind() != "call_expression" {
        return;
    }

    let Some(func) = expr.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }

    let Some(prop) = func.child_by_field_name("property") else { return };
    let Ok(method_name) = prop.utf8_text(source) else { return };

    if !PURE_METHODS.contains(&method_name) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-ignored-return".into(),
        message: format!(
            "Return value of `.{}` is ignored — the call has no side effect.",
            method_name
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
    fn flags_standalone_map() {
        let d = run_on("arr.map(x => x + 1);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(".map"));
    }

    #[test]
    fn flags_standalone_filter() {
        assert_eq!(run_on("items.filter(Boolean);").len(), 1);
    }

    #[test]
    fn allows_assigned_map() {
        assert!(run_on("const doubled = arr.map(x => x * 2);").is_empty());
    }

    #[test]
    fn allows_returned_map() {
        assert!(run_on("function f() { return arr.map(x => x * 2); }").is_empty());
    }

    #[test]
    fn flags_standalone_trim() {
        assert_eq!(run_on("name.trim();").len(), 1);
    }
}
