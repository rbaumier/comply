//! a11y-interactive-supports-focus AST backend.
//!
//! Elements with interactive handlers (`onClick`, `onKeyDown`) and an
//! interactive `role` must have `tabIndex` to be keyboard-focusable.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::jsx_attribute_name;

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "jsx_opening_element" && kind != "jsx_self_closing_element" {
        return;
    }

    let mut has_handler = false;
    let mut has_role = false;
    let mut has_tabindex = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match jsx_attribute_name(child, source) {
            Some("onClick" | "onKeyDown") => has_handler = true,
            Some("role") => has_role = true,
            Some("tabIndex" | "tabindex") => has_tabindex = true,
            _ => {}
        }
    }

    if has_handler && has_role && !has_tabindex {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-interactive-supports-focus".into(),
            message: "Element with interactive handler and `role` must have `tabIndex` to be focusable.".into(),
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
    fn flags_onclick_with_role_no_tabindex() {
        assert_eq!(run(r#"const x = <div onClick={handler} role="button" />;"#).len(), 1);
    }

    #[test]
    fn allows_onclick_with_role_and_tabindex() {
        assert!(run(r#"const x = <div onClick={handler} role="button" tabIndex={0} />;"#).is_empty());
    }

    #[test]
    fn allows_no_handler() {
        assert!(run(r#"const x = <div role="button" />;"#).is_empty());
    }
}
