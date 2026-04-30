//! prefer-negative-index backend — flag `.slice(x.length - N)` patterns.

use crate::diagnostic::{Diagnostic, Severity};

/// Methods where `.length - N` can be replaced with a negative index.
const METHODS: &[&str] = &["slice", "splice", "toSpliced", "at", "with", "subarray"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }

    let Some(property) = func.child_by_field_name("property") else { return };
    let method_name = property.utf8_text(source).unwrap_or("");
    if !METHODS.contains(&method_name) { return; }

    let Some(object) = func.child_by_field_name("object") else { return };
    let receiver = object.utf8_text(source).unwrap_or("");
    if receiver.is_empty() { return; }

    // Check arguments for `receiver.length - N` pattern.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    for i in 0..args.named_child_count() {
        let arg = args.named_child(i).unwrap();
        if is_length_minus(arg, source, receiver) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-negative-index".into(),
                message: "Prefer negative index over `.length - index`.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return; // one diagnostic per call
        }
    }
}

/// Check if an AST node represents `<receiver>.length - <expr>`.
fn is_length_minus(node: tree_sitter::Node, source: &[u8], receiver: &str) -> bool {
    if node.kind() != "binary_expression" {
        return false;
    }
    // Operator must be `-`
    let op_text = node.children(&mut node.walk()).find(|c| c.kind() == "-");
    if op_text.is_none() {
        return false;
    }

    let Some(left) = node.child_by_field_name("left") else {
        return false;
    };
    if left.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = left.child_by_field_name("object") else {
        return false;
    };
    let Some(prop) = left.child_by_field_name("property") else {
        return false;
    };

    let obj_text = obj.utf8_text(source).unwrap_or("");
    let prop_text = prop.utf8_text(source).unwrap_or("");

    obj_text == receiver && prop_text == "length"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_slice_length_minus() {
        let d = run_on("const x = str.slice(str.length - 3);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_splice_length_minus() {
        let d = run_on("arr.splice(arr.length - 1, 1);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_at_length_minus() {
        let d = run_on("const last = arr.at(arr.length - 1);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_negative_index() {
        assert!(run_on("const x = str.slice(-3);").is_empty());
    }

    #[test]
    fn allows_different_receiver() {
        assert!(run_on("const x = str.slice(other.length - 3);").is_empty());
    }

    #[test]
    fn allows_normal_slice() {
        assert!(run_on("const x = str.slice(0, 5);").is_empty());
    }
}
