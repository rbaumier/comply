//! intermediate-variables Rust backend.
//!
//! Flag deeply nested expressions that should be extracted into named
//! intermediate variables.

use crate::diagnostic::{Diagnostic, Severity};

fn count_operators(node: tree_sitter::Node) -> usize {
    let mut count = 0;
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if current.kind() == "binary_expression" { count += 1 }
        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    count
}

const OPERATOR_THRESHOLD: usize = 3;

crate::ast_check! { |node, source, ctx, diagnostics|
    let is_target = matches!(node.kind(), "return_expression" | "call_expression");
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
            message: "Expression is deeply nested — extract into named intermediate variables.".into(),
            severity: Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_complex_return() {
        let src = "fn f() -> i32 {\n  return a && b || c && d;\n}\n";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_simple_return() {
        let src = "fn f() -> i32 {\n  return a + b;\n}\n";
        assert!(run_on(src).is_empty());
    }
}
