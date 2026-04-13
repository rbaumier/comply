//! a11y-anchor-is-valid backend — AST-based detection.
//! a11y-anchor-is-valid backend — AST-based detection.
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_self_closing_element" && node.kind() != "jsx_opening_element" {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };
    if tag != "a" { return; }

    // Scan attributes for href
    let mut cursor = node.walk();
    let mut has_href = false;
    let mut href_value: Option<String> = None;

    for child in node.children(&mut cursor) {
        if crate::rules::jsx::jsx_attribute_name(child, source) != Some("href") { continue; }
        has_href = true;
        if let Some(val_node) = crate::rules::jsx::jsx_attribute_value(child) {
            let Ok(val_text) = val_node.utf8_text(source) else { continue };
            href_value = Some(val_text.to_string());
        }
    }

    let pos = node.start_position();

    if !has_href {
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-anchor-is-valid".into(),
            message: "Anchor is missing an `href` attribute.".into(),
            severity: Severity::Error,
            span: None,
        });
        return;
    }

    if let Some(val) = &href_value {
        if val == "\"#\"" || val == "'#'" {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "a11y-anchor-is-valid".into(),
                message: "Anchor has `href=\"#\"` — use a `<button>` or a real URL.".into(),
                severity: Severity::Error,
                span: None,
            });
        } else if val.contains("javascript:") {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "a11y-anchor-is-valid".into(),
                message: "Anchor has `href=\"javascript:\"` — use a `<button>` or a real URL.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_href_hash() {
        assert_eq!(run_on("const x = <a href=\"#\">Click</a>;").len(), 1);
    }

    #[test]
    fn flags_href_javascript() {
        assert_eq!(run_on(r#"const x = <a href="javascript:void(0)">Click</a>;"#).len(), 1);
    }

    #[test]
    fn flags_missing_href() {
        assert_eq!(run_on(r#"const x = <a onClick={handler}>Click</a>;"#).len(), 1);
    }

    #[test]
    fn allows_valid_href() {
        assert!(run_on(r#"const x = <a href="/home">Home</a>;"#).is_empty());
    }
}
