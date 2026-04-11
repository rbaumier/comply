//! ts-no-array-constructor backend — flag `new Array()` / `Array()` calls
//! without type arguments that have != 1 argument (matching the TS-eslint
//! rule which skips single-arg calls since `new Array(5)` for length is
//! sometimes intentional with type args).
//!
//! Skips calls with type arguments (`new Array<string>()`), which are
//! the TS-specific exception.

use crate::diagnostic::{Diagnostic, Severity};

fn is_array_callee(source: &[u8], node: tree_sitter::Node) -> bool {
    let callee_text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("");
    callee_text == "Array"
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "new_expression" && kind != "call_expression" {
        return;
    }

    // new_expression uses "constructor" field, call_expression uses "function".
    let callee = if kind == "new_expression" {
        node.child_by_field_name("constructor")
    } else {
        node.child_by_field_name("function")
    };
    let Some(callee) = callee else { return };
    if !is_array_callee(source, callee) {
        return;
    }

    // Skip if there are type arguments (TS-specific: `new Array<string>()`)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_arguments" {
            return;
        }
    }

    // Skip single-argument calls — `new Array(5)` for length pre-allocation.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let arg_count = args.named_child_count();
    if arg_count == 1 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-array-constructor".into(),
        message: "Use array literal `[]` instead of `Array()` constructor.".into(),
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
    fn flags_new_array_no_args() {
        let diags = run_on("const a = new Array();");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_new_array_multiple_args() {
        let diags = run_on("const a = new Array(1, 2, 3);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_single_arg() {
        assert!(run_on("const a = new Array(5);").is_empty());
    }

    #[test]
    fn allows_typed_array() {
        assert!(run_on("const a = new Array<string>();").is_empty());
    }
}
