//! react-jsx-no-bind AST backend.
//!
//! Flags JSX attribute values that are either:
//! - an arrow function: `onClick={() => ...}`
//! - a function expression: `onClick={function () {}}`
//! - a `.bind()` call: `onClick={this.handleClick.bind(this)}`
//!
//! These all produce a fresh reference per render and break memoization.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    let Some(value_node) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    // `{...}` wrappers are `jsx_expression` nodes — unwrap to the inner expression.
    if value_node.kind() != "jsx_expression" {
        return;
    }

    // Walk inside the expression braces to find the actual expression node.
    let mut inner: Option<tree_sitter::Node> = None;
    let mut cursor = value_node.walk();
    for child in value_node.children(&mut cursor) {
        match child.kind() {
            "{" | "}" => continue,
            _ => { inner = Some(child); break; }
        }
    }
    let Some(expr) = inner else { return };

    let (kind_label, reported_node) = match expr.kind() {
        "arrow_function" => ("arrow function", expr),
        "function_expression" | "function" => ("function expression", expr),
        "call_expression" => {
            // Detect `foo.bind(...)` — the callee is a member_expression ending in `.bind`.
            let Some(func) = expr.child_by_field_name("function") else { return };
            if func.kind() != "member_expression" { return; }
            let Some(prop) = func.child_by_field_name("property") else { return };
            let Ok(prop_name) = prop.utf8_text(source) else { return };
            if prop_name != "bind" { return; }
            ("`.bind()` call", expr)
        }
        _ => return,
    };

    let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    let pos = reported_node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "{kind_label} as value of JSX prop `{attr_name}` creates a new reference every render — hoist to `useCallback` or a stable handler."
        ),
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
    fn flags_arrow_in_prop() {
        let src = "const x = <button onClick={() => doThing()}>ok</button>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_function_expression_in_prop() {
        let src = "const x = <button onClick={function () { doThing(); }}>ok</button>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_bind_call_in_prop() {
        let src = "const x = <button onClick={this.handleClick.bind(this)}>ok</button>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_stable_handler_reference() {
        let src = "const x = <button onClick={handleClick}>ok</button>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_bind_call() {
        // Calling a function that returns a handler is outside this rule's scope,
        // and many codebases use this pattern intentionally.
        let src = "const x = <button onClick={makeHandler()}>ok</button>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_string_attribute() {
        let src = r#"const x = <div className="foo" />;"#;
        assert!(run_on(src).is_empty());
    }
}
