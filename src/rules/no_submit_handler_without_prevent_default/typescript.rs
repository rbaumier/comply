//! no-submit-handler-without-preventDefault AST backend.
//!
//! Inspect JSX attributes named `onSubmit`. If the value is an inline
//! arrow function / function expression, walk its body and ensure a
//! `preventDefault()` call appears somewhere.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

fn body_calls_prevent_default(body: Node, source: &[u8]) -> bool {
    let mut cursor = body.walk();
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression"
            && let Some(func) = node.child_by_field_name("function")
            && func.kind() == "member_expression"
            && let Some(prop) = func.child_by_field_name("property")
            && let Ok(prop_text) = prop.utf8_text(source)
            && prop_text == "preventDefault"
        {
            return true;
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

fn unwrap_jsx_expression(value: Node) -> Option<Node> {
    if value.kind() != "jsx_expression" {
        return None;
    }
    let mut cursor = value.walk();
    for child in value.children(&mut cursor) {
        if !matches!(child.kind(), "{" | "}") {
            return Some(child);
        }
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_attribute" {
        return;
    }

    let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    if attr_name != "onSubmit" {
        return;
    }

    let Some(value) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    let Some(expr) = unwrap_jsx_expression(value) else { return };

    // Only inspect inline handlers; referenced identifiers are out of scope.
    let body = match expr.kind() {
        "arrow_function" | "function" | "function_expression" => {
            let Some(b) = expr.child_by_field_name("body") else { return };
            b
        }
        _ => return,
    };

    if body_calls_prevent_default(body, source) {
        return;
    }

    let pos = expr.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`onSubmit` handler does not call `preventDefault()` — the browser will perform a full-page submit and reset the form.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_arrow_without_prevent_default() {
        let src = "const x = <form onSubmit={(e) => submit(e)}>ok</form>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_function_expression_without_prevent_default() {
        let src = "const x = <form onSubmit={function (e) { submit(e); }}>ok</form>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_arrow_with_prevent_default() {
        let src = "const x = <form onSubmit={(e) => { e.preventDefault(); submit(e); }}>ok</form>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_nested_prevent_default() {
        let src = r#"
const x = <form onSubmit={(e) => {
  if (valid) {
    e.preventDefault();
    submit(e);
  }
}}>ok</form>;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_referenced_handler() {
        // Cannot easily track across scopes; keep to inline handlers.
        let src = "const x = <form onSubmit={handleSubmit}>ok</form>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_other_attributes() {
        let src = "const x = <button onClick={(e) => submit(e)}>ok</button>;";
        assert!(run_on(src).is_empty());
    }
}
