//! prefer-dom-node-dataset backend — flag `.setAttribute('data-*')` etc.

use crate::diagnostic::{Diagnostic, Severity};

const METHODS: &[&str] = &["setAttribute", "getAttribute", "removeAttribute", "hasAttribute"];

/// Check if the first argument of a call is a string starting with `data-`.
fn first_arg_is_data_attr(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(args) = node.child_by_field_name("arguments") else { return false };
    let Some(first) = args.named_child(0) else { return false };
    if first.kind() != "string" {
        return false;
    }
    let text = first.utf8_text(source).unwrap_or("");
    // Strip quotes
    let inner = &text[1..text.len().saturating_sub(1)];
    inner.starts_with("data-")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }

    let Some(prop) = func.child_by_field_name("property") else { return };
    let prop_name = prop.utf8_text(source).unwrap_or("");

    if !METHODS.contains(&prop_name) {
        return;
    }

    if !first_arg_is_data_attr(node, source) {
        return;
    }

    let pos = prop.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-dom-node-dataset".into(),
        message: format!(
            "Prefer `.dataset` over `.{}(…)` for `data-*` attributes.",
            prop_name
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_set_attribute_data() {
        let d = run_on(r#"el.setAttribute('data-foo', 'bar');"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("setAttribute"));
    }

    #[test]
    fn flags_get_attribute_data() {
        let d = run_on(r#"const v = el.getAttribute("data-id");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("getAttribute"));
    }

    #[test]
    fn allows_non_data_attribute() {
        assert!(run_on(r#"el.setAttribute('class', 'active');"#).is_empty());
    }

    #[test]
    fn allows_dataset() {
        assert!(run_on(r#"el.dataset.foo = 'bar';"#).is_empty());
    }
}
