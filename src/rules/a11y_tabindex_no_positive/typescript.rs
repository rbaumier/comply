//! a11y-tabindex-no-positive AST backend.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a tabIndex attribute has a positive value (> 0).
fn is_positive_tabindex(attr: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(val) = crate::rules::jsx::jsx_attribute_value(attr) else { return false };
    let Ok(text) = val.utf8_text(source) else { return false };
    // JSX expression: {N} — extract the number
    let inner = text.strip_prefix('{').and_then(|s| s.strip_suffix('}'));
    if let Some(num_str) = inner
        && let Ok(n) = num_str.trim().parse::<i32>() {
            return n > 0;
        }
    // String literal: "N"
    let inner = text.strip_prefix('"').and_then(|s| s.strip_suffix('"'));
    if let Some(num_str) = inner
        && let Ok(n) = num_str.trim().parse::<i32>() {
            return n > 0;
        }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_opening_element" && node.kind() != "jsx_self_closing_element" {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name) = attr_name.utf8_text(source) else { continue };
        if name == "tabIndex" && is_positive_tabindex(child, source) {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "a11y-tabindex-no-positive".into(),
                message: "`tabIndex` must not be positive — use `0` or `-1` only.".into(),
                severity: Severity::Error,
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
    fn flags_positive_tabindex() {
        assert_eq!(run(r#"const x = <div tabIndex={5} />;"#).len(), 1);
    }

    #[test]
    fn flags_tabindex_1() {
        assert_eq!(run(r#"const x = <input tabIndex={1} />;"#).len(), 1);
    }

    #[test]
    fn allows_tabindex_zero() {
        assert!(run(r#"const x = <div tabIndex={0} />;"#).is_empty());
    }

    #[test]
    fn allows_tabindex_negative() {
        assert!(run(r#"const x = <div tabIndex={-1} />;"#).is_empty());
    }
}
