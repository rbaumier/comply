//! Flag icons inside a `<Button>` that don't carry the `data-icon`
//! attribute shadcn uses to size + space them via parent CSS.
//!
//! Two violations are reported:
//!   1. Legacy spacing classes — a child whose `className` includes
//!      `mr-2` / `ml-2`. `data-icon="inline-start"|"inline-end"`
//!      replaces this.
//!   2. Missing `data-icon` — a JSX child that *looks* like an icon
//!      (component name ends with `Icon`, or matches a known icon-library
//!      export like `ChevronRight`) but has no `data-icon` attribute.

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

/// Heuristic: does `tag` look like an icon component? Matches `*Icon`
/// (e.g. `<TrashIcon />`, `<UserIcon />`) and a small allow-list of
/// well-known lucide-react / heroicons exports that don't end with `Icon`
/// (`<ChevronRight />`, `<Plus />`, …).
fn looks_like_icon(tag: &str) -> bool {
    if tag.ends_with("Icon") && tag.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return true;
    }
    const KNOWN_ICONS: &[&str] = &[
        "ChevronLeft", "ChevronRight", "ChevronUp", "ChevronDown",
        "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown",
        "Plus", "Minus", "Check", "X", "Search", "Trash", "Edit", "Pencil",
        "Loader", "Spinner",
    ];
    KNOWN_ICONS.contains(&tag)
}

/// Does `child` carry a `data-icon` attribute (any value)?
fn has_data_icon_attr(child: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(attrs) = attributes_node(child, source) else { return false };
    let mut cursor = attrs.walk();
    for attr in attrs.children(&mut cursor) {
        if attr.kind() != "jsx_attribute" { continue; }
        if crate::rules::jsx::jsx_attribute_name(attr, source) == Some("data-icon") {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["jsx_element"] => |node, source, ctx, diagnostics|
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
            continue;
        }

        // Icon-shaped child with no data-icon attribute → flag it so the
        // parent button can size + space it via the shadcn `[data-icon]`
        // selector.
        let Some(child_tag) = opening_tag_name(child, source) else { continue };
        if looks_like_icon(child_tag) && !has_data_icon_attr(child, source) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &child,
                super::META.id,
                "Icon child of `<Button>` is missing a `data-icon` attribute — add `data-icon=\"inline-start\"` or `data-icon=\"inline-end\"`.".into(),
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
    fn flags_icon_without_data_icon_attr() {
        // `<Icon />` is icon-shaped but missing `data-icon` — still wrong
        // because the parent button can't size/space it via CSS.
        let src = r#"const x = <Button><Icon />Save</Button>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_lucide_named_icon_without_data_icon() {
        let src = r#"const x = <Button><ChevronRight />Next</Button>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_data_icon_on_lucide_named_icon() {
        let src = r#"const x = <Button><ChevronRight data-icon="inline-end" />Next</Button>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_button() {
        let src = r#"const x = <div><Icon className="mr-2" />hi</div>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_icon_child() {
        let src = r#"const x = <Button><span>Save</span></Button>;"#;
        assert!(run(src).is_empty());
    }
}
