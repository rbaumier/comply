//! a11y-heading-has-content AST backend.
//!
//! Walks JSX elements for `h1`–`h6` tags and flags self-closing or
//! empty headings that provide no text content for screen readers.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "jsx_self_closing_element" && kind != "jsx_opening_element" {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let Ok(tag) = name_node.utf8_text(source) else {
        return;
    };

    // Only care about h1–h6.
    if !matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
        return;
    }

    if kind == "jsx_self_closing_element" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-heading-has-content".into(),
            message: format!("`<{tag}>` is self-closing and has no content."),
            severity: Severity::Error,
        });
        return;
    }

    // jsx_opening_element — check the parent jsx_element for children.
    let Some(parent) = node.parent() else {
        return;
    };
    if parent.kind() != "jsx_element" {
        return;
    }

    // Check if the element has any meaningful children (text or nested elements).
    let mut has_content = false;
    let mut cursor = parent.walk();
    for child in parent.children(&mut cursor) {
        match child.kind() {
            "jsx_opening_element" | "jsx_closing_element" => {}
            "jsx_text" => {
                if let Ok(text) = child.utf8_text(source)
                    && !text.trim().is_empty() {
                        has_content = true;
                    }
            }
            _ => {
                has_content = true;
            }
        }
    }

    if !has_content {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-heading-has-content".into(),
            message: format!("`<{tag}>` is empty and has no content."),
            severity: Severity::Error,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_empty_h1() {
        assert_eq!(run("const x = <h1></h1>;").len(), 1);
    }

    #[test]
    fn flags_self_closing_h2() {
        assert_eq!(run("const x = <h2 />;").len(), 1);
    }

    #[test]
    fn flags_self_closing_h3_compact() {
        assert_eq!(run("const x = <h3/>;").len(), 1);
    }

    #[test]
    fn allows_heading_with_content() {
        assert!(run("const x = <h1>Welcome</h1>;").is_empty());
    }

    #[test]
    fn flags_empty_h6() {
        assert_eq!(run("const x = <h6></h6>;").len(), 1);
    }

    #[test]
    fn allows_heading_with_jsx_child() {
        assert!(run("const x = <h1><span>Ok</span></h1>;").is_empty());
    }
}
