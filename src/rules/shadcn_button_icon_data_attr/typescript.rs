//! Flag `mr-2` / `ml-2` on any JSX child inside a `<Button>` element.
//!
//! The heuristic fires when:
//!   1. We encounter a `jsx_element` whose opening tag is `Button`.
//!   2. One of its JSX element children has a `className` utility of
//!      `mr-2` or `ml-2`.
//!
//! The rule is intentionally narrow — the shadcn docs codify `mr-2`
//! and `ml-2` as the legacy anti-pattern that `data-icon` replaces.

use crate::diagnostic::{Diagnostic, Severity};

fn has_margin_icon_class(value: &str) -> bool {
    value.split_ascii_whitespace().any(|class| {
        let util = class.rsplit(':').next().unwrap_or(class).trim_start_matches('!');
        util == "mr-2" || util == "ml-2"
    })
}

fn opening_tag_name<'a>(elem: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    match elem.kind() {
        "jsx_element" => {
            let open = elem.child_by_field_name("open_tag")?;
            crate::rules::jsx::jsx_element_tag_name(open, source)
        }
        "jsx_self_closing_element" => crate::rules::jsx::jsx_element_tag_name(elem, source),
        _ => None,
    }
}

fn attributes_node<'a>(elem: tree_sitter::Node<'a>, _source: &'a [u8]) -> Option<tree_sitter::Node<'a>> {
    match elem.kind() {
        "jsx_element" => elem.child_by_field_name("open_tag"),
        "jsx_self_closing_element" => Some(elem),
        _ => None,
    }
}

fn child_has_offending_margin<'a>(child: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<tree_sitter::Node<'a>> {
    let attrs = attributes_node(child, source)?;
    let mut cursor = attrs.walk();
    for attr in attrs.children(&mut cursor) {
        if attr.kind() != "jsx_attribute" {
            continue;
        }
        if crate::rules::jsx::jsx_attribute_name(attr, source) != Some("className") {
            continue;
        }
        let Some(value) = crate::rules::jsx::jsx_attribute_string_value(attr, source) else {
            continue;
        };
        if has_margin_icon_class(value) {
            return Some(attr);
        }
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_element" {
        return;
    }
    let Some(tag) = opening_tag_name(node, source) else {
        return;
    };
    if tag != "Button" {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let child_kind = child.kind();
        if child_kind != "jsx_element" && child_kind != "jsx_self_closing_element" {
            continue;
        }
        if let Some(offending) = child_has_offending_margin(child, source) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &offending,
                super::META.id,
                "Icon inside `<Button>` uses `mr-2`/`ml-2` — replace with `data-icon=\"inline-start\"` or `data-icon=\"inline-end\"`.".into(),
                Severity::Warning,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_icon_with_mr_2() {
        let src = r#"const x = <Button><Icon className="mr-2" />Save</Button>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_icon_with_ml_2() {
        let src = r#"const x = <Button>Save<Icon className="ml-2" /></Button>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_data_icon_attribute() {
        let src = r#"const x = <Button><Icon data-icon="inline-start" />Save</Button>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_icon_without_margin() {
        let src = r#"const x = <Button><Icon />Save</Button>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_button() {
        let src = r#"const x = <div><Icon className="mr-2" />hi</div>;"#;
        assert!(run(src).is_empty());
    }
}
