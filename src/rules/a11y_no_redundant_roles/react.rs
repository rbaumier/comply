//! a11y-no-redundant-roles AST backend.

use crate::diagnostic::{Diagnostic, Severity};

/// (tag, redundant implicit role)
const REDUNDANT_PAIRS: &[(&str, &str)] = &[
    ("button", "button"),
    ("nav", "navigation"),
    ("img", "img"),
    ("input", "textbox"),
    ("h1", "heading"),
    ("h2", "heading"),
    ("h3", "heading"),
    ("h4", "heading"),
    ("h5", "heading"),
    ("h6", "heading"),
    ("ul", "list"),
    ("ol", "list"),
    ("li", "listitem"),
    ("table", "table"),
    ("form", "form"),
];

/// Extract the string value from a JSX attribute value node.
fn attr_string_value<'a>(attr: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    crate::rules::jsx::jsx_attribute_string_value(attr, source)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_opening_element" && node.kind() != "jsx_self_closing_element" {
        return;
    }

    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };

    // Collect attributes
    let mut role_value: Option<(String, tree_sitter::Node)> = None;
    let mut has_href = false;
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name) = attr_name.utf8_text(source) else { continue };
        if name == "role"
            && let Some(val) = attr_string_value(child, source) {
                role_value = Some((val.to_string(), child));
            }
        if name == "href" {
            has_href = true;
        }
    }

    let Some((role, _attr_node)) = role_value else { return };

    // Check standard redundant pairs
    for &(pair_tag, pair_role) in REDUNDANT_PAIRS {
        if tag == pair_tag && role == pair_role {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "a11y-no-redundant-roles".into(),
                message: format!(
                    "The element `<{tag}>` has an implicit role of `{pair_role}`. Setting `role=\"{pair_role}\"` is redundant."
                ),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }
    }

    // Special case: <a href="..." role="link"> is redundant
    if tag == "a" && has_href && role == "link" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-no-redundant-roles".into(),
            message: "The element `<a>` with `href` has an implicit role of `link`. Setting `role=\"link\"` is redundant.".into(),
            severity: Severity::Warning,
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
    fn flags_button_with_button_role() {
        let d = run(r#"const x = <button role="button">Click</button>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_nav_with_navigation_role() {
        let d = run(r#"const x = <nav role="navigation">Nav</nav>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_a_href_with_link_role() {
        let d = run(r#"const x = <a href="/page" role="link">Link</a>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_non_redundant_roles() {
        assert!(run(r#"const x = <div role="button">Click</div>;"#).is_empty());
    }
}
