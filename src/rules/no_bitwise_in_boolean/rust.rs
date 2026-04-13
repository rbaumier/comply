//! no-bitwise-in-boolean Rust backend.
//!
//! Flag bitwise ops (`&`, `|`, `^`) in boolean contexts (if/while conditions).

use crate::diagnostic::{Diagnostic, Severity};

const BITWISE_OPS: &[&str] = &["&", "|", "^"];

fn has_bitwise_op(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "binary_expression" => {
            if let Some(op) = node.child_by_field_name("operator") {
                let op_text = op.utf8_text(source).unwrap_or("");
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

crate::ast_check! { |node, source, ctx, diagnostics|
    let condition = match node.kind() {
        "if_expression" => node.child_by_field_name("condition"),
        "while_expression" => node.child_by_field_name("condition"),
        _ => return,
    };

    let Some(condition) = condition else { return };

    if has_bitwise_op(condition, source) {
        let pos = condition.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-bitwise-in-boolean".into(),
            message: "Bitwise operator in boolean context — did you mean `&&`/`||`?".into(),
            severity: Severity::Warning,
            span: None,
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
    fn flags_bitwise_in_if() {
        assert_eq!(run_on("fn f(a: bool, b: bool) { if a & b {} }").len(), 1);
    }

    #[test]
    fn allows_logical_and() {
        assert!(run_on("fn f(a: bool, b: bool) { if a && b {} }").is_empty());
    }
}
