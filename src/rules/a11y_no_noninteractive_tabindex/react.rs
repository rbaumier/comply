//! a11y-no-noninteractive-tabindex AST backend.

use crate::diagnostic::{Diagnostic, Severity};

const NON_INTERACTIVE: &[&str] = &["div", "span", "p", "section"];

/// Check if a tabIndex attribute has a value other than -1.
fn is_nonnegative_one_tabindex(attr: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(val) = crate::rules::jsx::jsx_attribute_value(attr) else {
        return true;
    };
    let Ok(text) = val.utf8_text(source) else {
        return true;
    };
    // {-1} or "-1" are OK
    text != "{-1}" && text != "\"-1\""
}

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };

    if !NON_INTERACTIVE.contains(&tag) {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name) = attr_name.utf8_text(source) else { continue };
        if name == "tabIndex" && is_nonnegative_one_tabindex(child, source) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "a11y-no-noninteractive-tabindex".into(),
                message: format!(
                    "Non-interactive element `<{tag}>` should not have `tabIndex`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_div_with_tabindex_zero() {
        let d = run(r#"const x = <div tabIndex={0}>Focusable div</div>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_div_with_tabindex_negative_one() {
        assert!(run(r#"const x = <div tabIndex={-1}>Not focusable</div>;"#).is_empty());
    }

    #[test]
    fn allows_button_with_tabindex() {
        assert!(run(r#"const x = <button tabIndex={0}>OK</button>;"#).is_empty());
    }

    #[test]
    fn flags_span_with_tabindex() {
        let d = run(r#"const x = <span tabIndex={1}>text</span>;"#);
        assert_eq!(d.len(), 1);
    }
}
