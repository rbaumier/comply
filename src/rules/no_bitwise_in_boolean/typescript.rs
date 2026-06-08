//! no-bitwise-in-boolean backend — flag bitwise ops in boolean contexts.

use crate::diagnostic::{Diagnostic, Severity};

/// Bitwise binary operators that are likely typos in boolean contexts.
const BITWISE_OPS: &[&str] = &["&", "|", "^", "~"];
const COMPARISON_OPS: &[&str] = &["==", "!=", "===", "!==", "<", ">", "<=", ">="];

/// Check whether a node is a bitwise binary or unary expression.
fn has_bitwise_op(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "binary_expression" => {
            if let Some(op) = node.child_by_field_name("operator") {
                let op_text = op.utf8_text(source).unwrap_or("");
                if COMPARISON_OPS.contains(&op_text) {
                    return false;
                }
                if BITWISE_OPS.contains(&op_text) {
                    return true;
                }
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if has_bitwise_op(child, source) {
                    return true;
                }
            }
            false
        }
        "unary_expression" => {
            if let Some(op) = node.child_by_field_name("operator") {
                let op_text = op.utf8_text(source).unwrap_or("");
                if op_text == "~" {
                    return true;
                }
            }
            false
        }
        "parenthesized_expression" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if has_bitwise_op(child, source) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

crate::ast_check! { on ["if_statement", "while_statement"] => |node, source, ctx, diagnostics|
    // Match if_statement, while_statement — check the condition child.
    let condition_field = match node.kind() {
        "if_statement" | "while_statement" => "condition",
        _ => return,
    };

    let Some(condition) = node.child_by_field_name(condition_field) else { return };

    // The condition is typically a parenthesized_expression; unwrap it.
    let inner = if condition.kind() == "parenthesized_expression" {
        condition.named_child(0).unwrap_or(condition)
    } else {
        condition
    };

    if !has_bitwise_op(inner, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-bitwise-in-boolean".into(),
        message: "Bitwise operator in boolean context — did you mean `&&` or `||`?".into(),
        severity: Severity::Warning,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_bitwise_and_in_if() {
        assert_eq!(run_on("if (x & y) {}").len(), 1);
    }

    #[test]
    fn flags_bitwise_or_in_if() {
        assert_eq!(run_on("if (x | y) {}").len(), 1);
    }

    #[test]
    fn flags_bitwise_xor_in_while() {
        assert_eq!(run_on("while (a ^ b) {}").len(), 1);
    }

    #[test]
    fn allows_logical_and() {
        assert!(run_on("if (x && y) {}").is_empty());
    }

    #[test]
    fn allows_logical_or() {
        assert!(run_on("if (x || y) {}").is_empty());
    }

    #[test]
    fn allows_bitwise_outside_condition() {
        assert!(run_on("const mask = a & b;").is_empty());
    }

    #[test]
    fn allows_bitmask_test() {
        assert!(run_on("if ((state & FLAG) === 0) {}").is_empty());
        assert!(run_on("while ((mask & bits) !== 0) {}").is_empty());
    }
}
