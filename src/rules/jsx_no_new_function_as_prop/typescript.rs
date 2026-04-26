//! jsx-no-new-function-as-prop AST backend.
//!
//! Flags `jsx_attribute` nodes whose value is a `jsx_expression` wrapping an
//! `arrow_function` or `function_expression`. Those allocate a fresh function
//! on every render.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    let Some(value_node) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    if value_node.kind() != "jsx_expression" {
        return;
    }

    // Walk inside the `{...}` braces to find the actual expression.
    let mut inner: Option<tree_sitter::Node> = None;
    let mut cursor = value_node.walk();
    for child in value_node.children(&mut cursor) {
        match child.kind() {
            "{" | "}" => continue,
            _ => { inner = Some(child); break; }
        }
    }
    let Some(expr) = inner else { return };

    let kind_label = match expr.kind() {
        "arrow_function" => "arrow function",
        "function_expression" | "function" => "function expression",
        _ => return,
    };

    let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    let pos = expr.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "{kind_label} as value of JSX prop `{attr_name}` creates a new reference every render — hoist with `useCallback` or to a stable handler."
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
    fn allows_stable_handler_reference() {
        let src = "const x = <button onClick={handleClick}>ok</button>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_member_expression_handler() {
        let src = "const x = <button onClick={obj.handler}>ok</button>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_string_attribute() {
        let src = r#"const x = <div className="foo" />;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_bind_call() {
        // `.bind()` is out of scope here (covered by `react-jsx-no-bind`).
        let src = "const x = <button onClick={handler.bind(this)}>ok</button>;";
        assert!(run_on(src).is_empty());
    }
}
