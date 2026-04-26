//! a11y-aria-unsupported-elements backend — AST-based detection.
use crate::diagnostic::{Diagnostic, Severity};

const UNSUPPORTED_ELEMENTS: &[&str] = &[
    "meta", "html", "script", "style", "head", "title", "link", "base",
];

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };

    if !UNSUPPORTED_ELEMENTS.contains(&tag) { return; }

    // Check if element has any aria-* attribute or role attribute
    let mut cursor = node.walk();
    let has_aria_or_role = node.children(&mut cursor).any(|child| {
        if child.kind() != "jsx_attribute" { return false; }
        let Some(attr_name) = child.child(0) else { return false };
        let Ok(name_text) = attr_name.utf8_text(source) else { return false };
        name_text.starts_with("aria-") || name_text == "role"
    });

    if has_aria_or_role {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-aria-unsupported-elements".into(),
            message: "ARIA attributes and `role` are not supported on this element.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_aria_on_meta() {
        assert_eq!(run_on(r#"const x = <meta aria-hidden="true" />;"#).len(), 1);
    }

    #[test]
    fn flags_role_on_script() {
        assert_eq!(run_on(r#"const x = <script role="presentation" />;"#).len(), 1);
    }

    #[test]
    fn allows_aria_on_div() {
        assert!(run_on(r#"const x = <div aria-label="hello" />;"#).is_empty());
    }
}
