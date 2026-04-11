//! intermediate-variables backend — flag deeply nested expressions that
//! should be extracted into named intermediate variables.

use crate::diagnostic::{Diagnostic, Severity};

/// Count binary/ternary operators in an expression subtree.
fn count_operators(node: tree_sitter::Node) -> usize {
    let mut count = 0;
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        match current.kind() {
            "binary_expression" | "augmented_assignment_expression" => count += 1,
            "ternary_expression" => count += 2, // ? and :
            "logical_expression" => count += 1, // && || ??
            _ => {}
        }
        cursor.reset(current);
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    count
}

const OPERATOR_THRESHOLD: usize = 3;

crate::ast_check! { |node, source, ctx, diagnostics|
    let is_target = matches!(node.kind(), "return_statement" | "call_expression");

    if !is_target {
        return;
    }

    if count_operators(node) >= OPERATOR_THRESHOLD {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "intermediate-variables".into(),
            message: "Expression is deeply nested — extract sub-expressions into named intermediate variables.".into(),
            severity: Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_complex_return() {
        let src = "function f() {\n  return a && b || c ?? d;\n}\n";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_simple_return() {
        let src = "function f() {\n  return a + b;\n}\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_complex_function_call() {
        let src = "doSomething(a + b * c / d);\n";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_simple_call() {
        let src = "doSomething(a, b);\n";
        assert!(run_on(src).is_empty());
    }
}
