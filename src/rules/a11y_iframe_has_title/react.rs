//! a11y-iframe-has-title AST backend.
//!
//! Flags `<iframe>` elements that are missing a `title` attribute.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::jsx_attribute_name;

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let Ok(tag) = name_node.utf8_text(source) else {
        return;
    };
    if tag != "iframe" {
        return;
    }

    let mut cursor = node.walk();
    let has_title = node.children(&mut cursor).any(|child| {
        jsx_attribute_name(child, source) == Some("title")
    });

    if !has_title {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-iframe-has-title".into(),
            message: "`<iframe>` is missing a `title` attribute.".into(),
            severity: Severity::Error,
            span: None,
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
    fn flags_iframe_without_title() {
        assert_eq!(run(r#"const x = <iframe src="https://example.com" />;"#).len(), 1);
    }

    #[test]
    fn allows_iframe_with_title() {
        assert!(run(r#"const x = <iframe src="https://example.com" title="Example" />;"#).is_empty());
    }

    #[test]
    fn flags_iframe_opening_without_title() {
        assert_eq!(run(r#"const x = <iframe src="https://example.com"></iframe>;"#).len(), 1);
    }
}
