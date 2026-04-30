//! a11y-aria-activedescendant-has-tabindex backend — AST-based detection.
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    // Check if this element has aria-activedescendant
    let mut cursor = node.walk();
    let has_aria_ad = node.children(&mut cursor).any(|child| {
        if child.kind() != "jsx_attribute" { return false; }
        let Some(attr_name) = child.child(0) else { return false };
        let Ok(name_text) = attr_name.utf8_text(source) else { return false };
        name_text == "aria-activedescendant"
    });

    if !has_aria_ad { return; }

    // Check if this element has tabIndex
    let mut cursor2 = node.walk();
    let has_tabindex = node.children(&mut cursor2).any(|child| {
        if child.kind() != "jsx_attribute" { return false; }
        let Some(attr_name) = child.child(0) else { return false };
        let Ok(name_text) = attr_name.utf8_text(source) else { return false };
        name_text == "tabIndex" || name_text == "tabindex"
    });

    if !has_tabindex {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-aria-activedescendant-has-tabindex".into(),
            message: "Element with `aria-activedescendant` must have `tabIndex` to be tabbable.".into(),
            severity: Severity::Error,
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
    fn flags_missing_tabindex() {
        assert_eq!(
            run_on(r#"const x = <div aria-activedescendant="item-1" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_with_tabindex() {
        assert!(
            run_on(r#"const x = <div aria-activedescendant="item-1" tabIndex={0} />;"#).is_empty()
        );
    }

    #[test]
    fn allows_tabindex_multiline() {
        let src = "const x = <div\n  aria-activedescendant=\"item-1\"\n  tabIndex={0}\n/>;";
        assert!(run_on(src).is_empty());
    }
}
