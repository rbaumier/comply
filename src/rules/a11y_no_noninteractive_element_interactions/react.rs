//! a11y-no-noninteractive-element-interactions AST backend.

use crate::diagnostic::{Diagnostic, Severity};

const NON_INTERACTIVE: &[&str] = &[
    "div", "span", "p", "section", "article", "header", "footer", "main", "aside", "nav",
];

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };

    if !NON_INTERACTIVE.contains(&tag) {
        return;
    }

    let mut has_handler = false;
    let mut has_role = false;
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name) = attr_name.utf8_text(source) else { continue };
        if name == "role" {
            has_role = true;
        }
        if name == "onClick" || name == "onKeyDown" {
            has_handler = true;
        }
    }

    if has_handler && !has_role {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-no-noninteractive-element-interactions".into(),
            message: format!(
                "Non-interactive element `<{tag}>` has an event handler without a `role` attribute."
            ),
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
    fn flags_div_with_onclick_no_role() {
        let d = run(r#"const x = <div onClick={handler}>Click me</div>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_section_with_onkeydown_no_role() {
        let d = run(r#"const x = <section onKeyDown={handler}>Content</section>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_div_with_onclick_and_role() {
        assert!(run(r#"const x = <div role="button" onClick={handler}>Click</div>;"#).is_empty());
    }

    #[test]
    fn allows_button_with_onclick() {
        assert!(run(r#"const x = <button onClick={handler}>Click</button>;"#).is_empty());
    }
}
