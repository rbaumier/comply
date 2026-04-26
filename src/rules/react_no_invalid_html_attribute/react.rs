//! react-no-invalid-html-attribute AST backend.
//!
//! Flags invalid values in the `rel` attribute on `<a>` and `<link>` elements.

use crate::diagnostic::{Diagnostic, Severity};

/// Valid `rel` values for `<a>` elements.
const VALID_A_RELS: &[&str] = &[
    "alternate", "author", "bookmark", "external", "help", "license",
    "next", "nofollow", "noopener", "noreferrer", "opener", "prev",
    "search", "tag", "ugc", "sponsored",
];

/// Valid `rel` values for `<link>` elements.
const VALID_LINK_RELS: &[&str] = &[
    "alternate", "author", "canonical", "dns-prefetch", "help", "icon",
    "license", "manifest", "modulepreload", "next", "pingback",
    "preconnect", "prefetch", "preload", "prerender", "prev", "search",
    "shortlink", "stylesheet", "apple-touch-icon",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_self_closing_element" && node.kind() != "jsx_opening_element" {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };

    let valid_rels = match tag {
        "a" => VALID_A_RELS,
        "link" => VALID_LINK_RELS,
        _ => return,
    };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name_text) = attr_name.utf8_text(source) else { continue };
        if name_text != "rel" {
            continue;
        }
        let Some(val_node) = child.child(2) else { continue };
        if val_node.kind() != "string" {
            continue;
        }
        let Ok(val) = val_node.utf8_text(source) else { continue };
        let unquoted = val.trim_matches(|c| c == '"' || c == '\'');

        for token in unquoted.split_whitespace() {
            if !valid_rels.contains(&token) {
                let pos = child.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "react-no-invalid-html-attribute".into(),
                    message: format!(
                        "Invalid `rel` value `{token}` on `<{tag}>`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
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
    fn flags_invalid_rel_on_anchor() {
        let src = r#"const x = <a rel="foobar">link</a>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_valid_rel_on_anchor() {
        let src = r#"const x = <a rel="noopener noreferrer">link</a>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_invalid_rel_on_link() {
        let src = r#"const x = <link rel="invalid" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_valid_rel_on_link() {
        let src = r#"const x = <link rel="stylesheet" />;"#;
        assert!(run(src).is_empty());
    }
}
