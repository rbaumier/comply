//! no-array-reduce backend — flag `.reduce()` / `.reduceRight()` calls.

use crate::diagnostic::{Diagnostic, Severity};

const METHODS: &[&str] = &["reduce", "reduceRight"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");
    if !METHODS.contains(&method) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-array-reduce".into(),
        message: format!(
            "`Array#{}()` is not allowed — use a `for` loop or other array methods for better readability.",
            method
        ),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_reduce() {
        assert_eq!(run_on("const sum = arr.reduce((acc, x) => acc + x, 0);").len(), 1);
    }

    #[test]
    fn flags_reduce_right() {
        assert_eq!(run_on("const r = arr.reduceRight((acc, x) => acc + x, 0);").len(), 1);
    }

    #[test]
    fn allows_non_reduce() {
        assert!(run_on("const x = arr.map(x => x * 2);").is_empty());
    }

    #[test]
    fn allows_direct_function_call() {
        assert!(run_on("reduce(acc, x);").is_empty());
    }
}
