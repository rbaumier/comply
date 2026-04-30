//! AstCheck: walk JSX elements (`button`, `a`, or anything with
//! `role="button"`) and flag classes that start with `-z-`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value, jsx_element_tag_name};

fn is_interactive_tag(tag: &str) -> bool {
    matches!(tag, "button" | "a")
}

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] =>
    |node, source, ctx, diagnostics|
    let Some(tag) = jsx_element_tag_name(node, source) else { return; };

    let mut role_button = false;
    let mut neg_z_class: Option<String> = None;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(name) = jsx_attribute_name(child, source) else { continue; };
        match name {
            "role" => {
                if jsx_attribute_string_value(child, source) == Some("button") {
                    role_button = true;
                }
            }
            "className" | "class" => {
                if let Some(value) = jsx_attribute_string_value(child, source)
                    && let Some(c) = value.split_whitespace().find(|c| c.starts_with("-z-"))
                {
                    neg_z_class = Some(c.to_string());
                }
            }
            _ => {}
        }
    }

    let interactive = is_interactive_tag(tag) || role_button;
    if !interactive { return; }
    let Some(klass) = neg_z_class else { return; };

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`<{tag}>` has `{klass}` — negative z-index sends interactive elements behind their stacking context and blocks clicks."
        ),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_button_with_negative_z() {
        assert_eq!(run(r#"const x = <button className="-z-10" />;"#).len(), 1);
    }

    #[test]
    fn flags_anchor_with_negative_z() {
        assert_eq!(
            run(r#"const x = <a href="/h" className="-z-1">x</a>;"#).len(),
            1
        );
    }

    #[test]
    fn flags_role_button_div() {
        assert_eq!(
            run(r#"const x = <div role="button" className="-z-50" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_button_without_negative_z() {
        assert!(run(r#"const x = <button className="z-10" />;"#).is_empty());
    }

    #[test]
    fn allows_div_with_negative_z() {
        assert!(run(r#"const x = <div className="-z-10" />;"#).is_empty());
    }
}
