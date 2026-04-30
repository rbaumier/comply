//! array-callback-without-return AST backend — flag array method callbacks
//! with block body `=> { ... }` but no `return` statement.

use crate::diagnostic::{Diagnostic, Severity};

const ARRAY_METHODS: &[&str] = &[
    "map", "filter", "reduce", "find", "some", "every", "flatMap",
];

/// Check whether a node is a `member_expression` calling one of the array
/// methods we care about: `x.map(`, `x.filter(` etc.
fn is_array_method_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    let name = prop.utf8_text(source).unwrap_or("");
    ARRAY_METHODS.contains(&name)
}

/// Check whether a subtree contains a `return_statement`.
fn has_return(node: tree_sitter::Node) -> bool {
    if node.kind() == "return_statement" {
        return true;
    }
    // Don't descend into nested functions — their returns don't count.
    if matches!(
        node.kind(),
        "function_declaration"
            | "function"
            | "arrow_function"
            | "method_definition"
            | "generator_function"
            | "generator_function_declaration"
    ) {
        return false;
    }
    let count = node.child_count();
    for i in 0..count {
        if has_return(node.child(i).unwrap()) {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_array_method_call(node, source) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };

    // The first argument to the array method is the callback.
    let Some(callback) = args.named_child(0) else { return };

    // We only care about arrow functions with a block body.
    if callback.kind() != "arrow_function" {
        return;
    }
    let Some(body) = callback.child_by_field_name("body") else { return };
    if body.kind() != "statement_block" {
        // Concise arrow `=> expr` — always has an implicit return.
        return;
    }

    if !has_return(body) {
        let pos = callback.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "array-callback-without-return".into(),
            message: "Array method callback uses block body `=> { ... }` without a `return` statement.".into(),
            severity: Severity::Error,
            span: None,
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
    fn flags_map_without_return() {
        let src = r#"const x = arr.map((item) => {
  console.log(item);
});"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_filter_without_return() {
        let src = r#"const x = arr.filter((item) => {
  item > 0;
});"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_map_with_return() {
        let src = r#"const x = arr.map((item) => {
  return item * 2;
});"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_concise_arrow() {
        let src = "const x = arr.map((item) => item * 2);";
        assert!(run_on(src).is_empty());
    }
}
