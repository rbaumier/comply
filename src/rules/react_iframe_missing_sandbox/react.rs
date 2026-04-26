//! react-iframe-missing-sandbox backend — `<iframe>` without `sandbox` attribute.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };

    if tag != "iframe" {
        return;
    }

    // Check if sandbox attribute exists
    let mut cursor = node.walk();
    let has_sandbox = node.children(&mut cursor).any(|child| {
        if child.kind() != "jsx_attribute" {
            return false;
        }
        let Some(attr_name) = child.child(0) else { return false };
        let Ok(name_text) = attr_name.utf8_text(source) else { return false };
        name_text == "sandbox"
    });

    if !has_sandbox {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-iframe-missing-sandbox".into(),
            message: "`<iframe>` without a `sandbox` attribute can access \
                      the parent page. Add `sandbox` to restrict its \
                      capabilities."
                .into(),
            severity: Severity::Warning,
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
    fn flags_iframe_without_sandbox() {
        let src = r#"const x = <iframe src="https://example.com" />;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_iframe_with_sandbox() {
        let src = r#"const x = <iframe src="https://example.com" sandbox="allow-scripts" />;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_iframe_with_empty_sandbox() {
        let src = r#"const x = <iframe src="https://example.com" sandbox="" />;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_iframe() {
        let src = r#"const x = <div src="https://example.com" />;"#;
        assert!(run_on(src).is_empty());
    }
}
