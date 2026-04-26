//! react-jsx-no-new-array-as-prop AST backend.
//!
//! Flags `jsx_attribute` nodes whose value is a `jsx_expression` that
//! directly contains an `array` literal — e.g. `items={[1, 2, 3]}`.

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

    if expr.kind() != "array" {
        return;
    }

    let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    let pos = expr.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "array literal as value of JSX prop `{attr_name}` creates a new reference every render — extract to a constant or use `useMemo`."
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
    fn flags_number_array_literal_in_prop() {
        let src = "const x = <Comp items={[1, 2, 3]} />;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_string_array_literal_in_prop() {
        let src = "const x = <Comp options={['a', 'b']} />;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_empty_array_literal_in_prop() {
        let src = "const x = <Comp items={[]} />;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_identifier_prop_value() {
        let src = "const x = <Comp items={items} />;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_function_call_returning_array() {
        let src = "const x = <Comp items={getItems()} />;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_string_attribute() {
        let src = r#"const x = <div className="foo" />;"#;
        assert!(run_on(src).is_empty());
    }
}
