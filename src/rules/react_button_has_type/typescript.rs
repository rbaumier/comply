//! react-button-has-type AST backend.
//!
//! Flags `<button>` elements that lack an explicit `type` attribute.

use crate::diagnostic::{Diagnostic, Severity};

const VALID_TYPES: &[&str] = &["button", "submit", "reset"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_self_closing_element" && node.kind() != "jsx_opening_element" {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };

    if tag != "button" {
        return;
    }

    let mut cursor = node.walk();
    let mut has_valid_type = false;

    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name_text) = attr_name.utf8_text(source) else { continue };
        if name_text == "type" {
            // Check value if present
            if let Some(val_node) = child.child(2) {
                if val_node.kind() == "string" {
                    let Ok(val) = val_node.utf8_text(source) else { continue };
                    let unquoted = val.trim_matches(|c| c == '"' || c == '\'');
                    if VALID_TYPES.contains(&unquoted) {
                        has_valid_type = true;
                    }
                } else {
                    // Dynamic expression — assume valid
                    has_valid_type = true;
                }
            } else {
                // Bare `type` attribute — treat as present
                has_valid_type = true;
            }
        }
    }

    if !has_valid_type {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-button-has-type".into(),
            message: "`<button>` missing an explicit `type` attribute — \
                      defaults to `submit`, which may cause unexpected \
                      form submissions."
                .into(),
            severity: Severity::Warning,
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
    fn flags_button_without_type() {
        let src = r#"const x = <button>Click</button>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_button_with_type() {
        let src = r#"const x = <button type="button">Click</button>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_submit_type() {
        let src = r#"const x = <button type="submit">Go</button>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_invalid_type_value() {
        let src = r#"const x = <button type="invalid">Go</button>;"#;
        assert_eq!(run(src).len(), 1);
    }
}
