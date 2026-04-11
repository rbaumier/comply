//! react-checked-requires-onchange backend — checked without onChange or readOnly.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a JSX element has a given attribute name.
fn has_jsx_attr(node: tree_sitter::Node, source: &[u8], attr_name: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(name_node) = child.child(0) else { continue };
        let Ok(name) = name_node.utf8_text(source) else { continue };
        if name == attr_name {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_self_closing_element" && node.kind() != "jsx_opening_element" {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };

    if tag != "input" {
        return;
    }

    // Must have `checked` but NOT `defaultChecked`
    if !has_jsx_attr(node, source, "checked") {
        return;
    }
    if has_jsx_attr(node, source, "defaultChecked") {
        return;
    }

    // Must be missing both onChange and readOnly
    if has_jsx_attr(node, source, "onChange") || has_jsx_attr(node, source, "readOnly") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-checked-requires-onchange".into(),
        message: "`checked` without `onChange` or `readOnly` renders \
                  a frozen input. Add an `onChange` handler or \
                  `readOnly`."
            .into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_checked_without_onchange() {
        let src = r#"const x = <input type="checkbox" checked={isChecked} />;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_checked_with_onchange() {
        let src = r#"const x = <input type="checkbox" checked={isChecked} onChange={handleChange} />;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_checked_with_readonly() {
        let src = r#"const x = <input type="checkbox" checked={isChecked} readOnly />;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_default_checked() {
        let src = r#"const x = <input type="checkbox" defaultChecked={true} />;"#;
        assert!(run_on(src).is_empty());
    }
}
