//! a11y-prefer-tag-over-role AST backend.

use crate::diagnostic::{Diagnostic, Severity};

/// (role value, suggested element)
const ROLE_TO_TAG: &[(&str, &str)] = &[
    ("button", "<button>"),
    ("link", "<a>"),
    ("img", "<img>"),
    ("heading", "<h1>-<h6>"),
    ("navigation", "<nav>"),
    ("banner", "<header>"),
    ("contentinfo", "<footer>"),
    ("main", "<main>"),
];

const GENERIC_ELEMENTS: &[&str] = &["div", "span"];

/// Extract the string value from a JSX attribute value node.
fn attr_string_value<'a>(attr: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    crate::rules::jsx::jsx_attribute_string_value(attr, source)
}

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };

    if !GENERIC_ELEMENTS.contains(&tag) {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name) = attr_name.utf8_text(source) else { continue };
        if name != "role" {
            continue;
        }
        if let Some(role) = attr_string_value(child, source) {
            for &(mapped_role, suggested) in ROLE_TO_TAG {
                if role == mapped_role {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "a11y-prefer-tag-over-role".into(),
                        message: format!(
                            "Prefer `{suggested}` over `<{tag} role=\"{role}\">` for semantic HTML."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
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
    fn flags_div_role_button() {
        let d = run(r#"const x = <div role="button">Click</div>;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("<button>"));
    }

    #[test]
    fn flags_span_role_img() {
        let d = run(r#"const x = <span role="img">icon</span>;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("<img>"));
    }

    #[test]
    fn flags_div_role_navigation() {
        let d = run(r#"const x = <div role="navigation">Nav</div>;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("<nav>"));
    }

    #[test]
    fn allows_button_element() {
        assert!(run(r#"const x = <button>Click</button>;"#).is_empty());
    }
}
