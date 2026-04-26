//! react-jsx-no-new-object-as-prop AST backend.
//!
//! Flags `jsx_attribute` nodes whose value is a `jsx_expression` wrapping an
//! inline `object` literal — `<Comp style={{ color: 'red' }} />`. Identifier
//! references (`style={styles}`) and calls (`useMemo(() => ({...}), [])`) are
//! allowed because they can carry a stable reference across renders.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    let Some(value_node) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    if value_node.kind() != "jsx_expression" {
        return;
    }

    // Unwrap `{ ... }` to the first meaningful child.
    let mut inner: Option<tree_sitter::Node> = None;
    let mut cursor = value_node.walk();
    for child in value_node.children(&mut cursor) {
        match child.kind() {
            "{" | "}" => continue,
            _ => { inner = Some(child); break; }
        }
    }
    let Some(expr) = inner else { return };

    if expr.kind() != "object" {
        return;
    }

    let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    let pos = expr.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Object literal as value of JSX prop `{attr_name}` creates a new reference every render — extract to a constant or wrap in `useMemo`."
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
    fn flags_inline_style_object() {
        let src = "const x = <Comp style={{ color: 'red' }} />;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_inline_config_object() {
        let src = "const x = <Comp config={{ a: 1 }} />;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_identifier_reference() {
        let src = "const x = <Comp style={styles} />;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_usememo_call() {
        let src = "const x = <Comp style={useMemo(() => ({ color }), [color])} />;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_string_attribute() {
        let src = r#"const x = <div className="foo" />;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_number_literal() {
        let src = "const x = <Comp count={42} />;";
        assert!(run_on(src).is_empty());
    }
}
