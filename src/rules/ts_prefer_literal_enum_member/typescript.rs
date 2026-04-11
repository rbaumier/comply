//! ts-prefer-literal-enum-member backend — check enum members for
//! non-literal initializers.
//!
//! Tree-sitter node structure:
//!   enum_declaration > enum_body > enum_assignment { name, value }

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a node is a literal value (string, number, template without
/// expressions, or a unary +/- on a number).
fn is_literal(node: tree_sitter::Node) -> bool {
    match node.kind() {
        "number" | "string" | "true" | "false" | "null" => true,
        "template_string" => {
            // Only literal if no template_substitution children
            let mut cursor = node.walk();
            !node
                .named_children(&mut cursor)
                .any(|c| c.kind() == "template_substitution")
        }
        "unary_expression" => {
            // Allow +N and -N
            let Some(op) = node.child(0) else {
                return false;
            };
            let op_kind = op.kind();
            if op_kind != "+" && op_kind != "-" {
                return false;
            }
            node.named_children(&mut node.walk())
                .any(|c| c.kind() == "number")
        }
        _ => false,
    }
}

crate::ast_check! { |node, _source, ctx, diagnostics|
    if node.kind() != "enum_assignment" {
        return;
    }
    let Some(value_node) = node.child_by_field_name("value") else {
        // No initializer — that's fine (auto-increment).
        return;
    };
    if is_literal(value_node) {
        return;
    }
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let pos = name_node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-prefer-literal-enum-member".into(),
        message: "Enum member should be initialized with a literal value \
                  (string or number), not a computed expression."
            .into(),
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
    fn allows_string_literal() {
        assert!(run_on(r#"enum E { A = "hello" }"#).is_empty());
    }

    #[test]
    fn allows_number_literal() {
        assert!(run_on("enum E { A = 1 }").is_empty());
    }

    #[test]
    fn allows_no_initializer() {
        assert!(run_on("enum E { A, B, C }").is_empty());
    }

    #[test]
    fn flags_computed_expression() {
        let diags = run_on("enum E { A = getValue() }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_reference_to_variable() {
        let diags = run_on("const x = 1; enum E { A = x }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_negative_number() {
        assert!(run_on("enum E { A = -1 }").is_empty());
    }
}
