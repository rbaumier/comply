//! a11y-click-events-have-key-events oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXElementName, JSXMemberExpressionObject,
};
use std::sync::Arc;

pub struct Check;

/// Native HTML tags with built-in keyboard activation — `Space`/`Enter`
/// fire `click` automatically, so adding `onKeyDown` is redundant.
const NATIVE_INTERACTIVE_TAGS: &[&str] =
    &["button", "a", "input", "select", "textarea", "summary", "details"];

/// Component-name roots whose `*Item` descendants expose a native
/// `menuitem`/`option`-style role (full keyboard + typeahead navigation by
/// construction). Matched together with the `Item` suffix so only items of an
/// interactive container are exempt — `ListItem`/`GridItem`/`AccordionItem`
/// stay flagged.
const INTERACTIVE_ITEM_CONTAINERS: &[&str] = &[
    "Menu",
    "Select",
    "Combobox",
    "Listbox",
    "Command",
    "Dropdown",
    "Autocomplete",
];

fn tag_has_native_keyboard_support(tag: &str) -> bool {
    // Lowercase identifier = native HTML tag.
    if NATIVE_INTERACTIVE_TAGS.contains(&tag) {
        return true;
    }
    // Uppercase identifier = component. Treat names ending in `Button`
    // (Button, IconButton, PrimaryButton, …) as wrapping a real
    // `<button>` — same keyboard semantics apply.
    if tag.ends_with("Button") {
        return true;
    }
    // Link components (Link, NavLink, RouterLink, …) render an `<a href>`;
    // Enter activates them natively, like the native `<a>` above.
    if tag.ends_with("Link") {
        return true;
    }
    // Menu/select/listbox/combobox item components carry a `menuitem`/`option`
    // role with built-in keyboard navigation (DropdownMenuItem, SelectItem,
    // ContextMenuItem, DropdownMenuRadioItem, …). `Option` is the bare case.
    if tag == "Option" {
        return true;
    }
    tag.ends_with("Item") && INTERACTIVE_ITEM_CONTAINERS.iter().any(|root| tag.contains(root))
}

/// Member-expression form of [`tag_has_native_keyboard_support`] — the
/// namespaced component API (`<DropdownMenu.Item>`, `<NavigationMenu.Link>`,
/// `<Select.Item>`). Exempt when the object root is an interactive container
/// and the property is an item/option, or the property is a link.
fn member_tag_has_native_keyboard_support(object: &str, property: &str) -> bool {
    if property.ends_with("Link") {
        return true;
    }
    let is_item_property =
        property == "Option" || property.ends_with("Item") || property.ends_with("Option");
    is_item_property && INTERACTIVE_ITEM_CONTAINERS.iter().any(|root| object.contains(root))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        let exempt = match &opening.name {
            JSXElementName::Identifier(id) => tag_has_native_keyboard_support(&id.name),
            JSXElementName::IdentifierReference(id) => tag_has_native_keyboard_support(&id.name),
            JSXElementName::NamespacedName(ns) => tag_has_native_keyboard_support(&ns.name.name),
            JSXElementName::MemberExpression(m) => {
                // Object root identifier, e.g. `DropdownMenu` in `<DropdownMenu.Item>`.
                let object = match &m.object {
                    JSXMemberExpressionObject::IdentifierReference(id) => Some(id.name.as_str()),
                    _ => None,
                };
                object.is_some_and(|object| {
                    member_tag_has_native_keyboard_support(object, &m.property.name)
                })
            }
            _ => false,
        };
        if exempt {
            return;
        }

        let mut has_onclick = false;
        let mut has_key_handler = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            match name_ident.name.as_str() {
                "onClick" => has_onclick = true,
                "onKeyDown" | "onKeyUp" | "onKeyPress" => has_key_handler = true,
                _ => {}
            }
        }

        if has_onclick && !has_key_handler {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Element has `onClick` without a corresponding keyboard event handler (`onKeyDown`/`onKeyUp`/`onKeyPress`).".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_div_with_onclick_no_key_handler() {
        let src = r#"const x = <div onClick={handler}>Click</div>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_div_with_onclick_and_key_handler() {
        let src = r#"const x = <div onClick={h} onKeyDown={h}>Click</div>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_native_button() {
        let src = r#"const x = <button onClick={handler}>Click</button>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_capitalized_button_component() {
        // Regression for rbaumier/comply#15 — <Button onClick> should not fire.
        let src = r#"const x = <Button size="sm" onClick={() => setOpen(true)}>Nouvel utilisateur</Button>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_named_button_components() {
        for tag in ["IconButton", "PrimaryButton", "DangerButton"] {
            let src = format!("const x = <{tag} onClick={{h}}>x</{tag}>;");
            assert!(run(&src).is_empty(), "{tag} should be ignored");
        }
    }

    #[test]
    fn ignores_native_anchor_input_select() {
        for tag in ["a", "input", "select", "textarea"] {
            let src = format!("const x = <{tag} onClick={{h}} />;");
            assert!(run(&src).is_empty(), "<{tag}> should be ignored");
        }
    }

    #[test]
    fn ignores_link_component() {
        // Regression for rbaumier/comply#4073 — TanStack <Link> renders <a href>.
        let src = r#"const x = <Link to={href} onClick={handleClick}>Retour</Link>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_dropdown_menu_item() {
        // Regression for rbaumier/comply#4073 — Base UI <DropdownMenuItem> has
        // full keyboard + typeahead navigation built in.
        let src =
            r#"const x = <DropdownMenuItem onClick={() => setEditing(true)}>Modifier</DropdownMenuItem>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_link_and_item_families() {
        for tag in ["NavLink", "SelectItem", "ContextMenuItem", "DropdownMenuRadioItem"] {
            let src = format!("const x = <{tag} onClick={{h}}>x</{tag}>;");
            assert!(run(&src).is_empty(), "<{tag}> should be ignored");
        }
    }

    #[test]
    fn ignores_namespaced_item_and_link_components() {
        // The namespaced API (Base UI / Radix) — `<DropdownMenu.Item>`,
        // `<Select.Item>`, `<NavigationMenu.Link>` — is exempt like the flat form.
        for (object, property) in [
            ("DropdownMenu", "Item"),
            ("Select", "Item"),
            ("ContextMenu", "RadioItem"),
            ("NavigationMenu", "Link"),
        ] {
            let src = format!(
                "const x = <{object}.{property} onClick={{h}}>x</{object}.{property}>;"
            );
            assert!(run(&src).is_empty(), "<{object}.{property}> should be ignored");
        }
    }

    #[test]
    fn flags_non_interactive_namespaced_item() {
        // A non-interactive container's namespaced item still flags.
        let src = r#"const x = <List.Item onClick={h}>x</List.Item>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_deeply_nested_namespaced_item() {
        // A multi-segment chain (object is itself a member expression) can't be
        // resolved to an interactive container root, so it flags.
        let src = r#"const x = <Foo.Bar.Item onClick={h}>x</Foo.Bar.Item>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_non_interactive_item_and_generic_elements() {
        // The bound must stay tight: non-interactive `*Item` components and
        // generic elements still require a keyboard handler.
        for tag in ["div", "span", "ListItem", "GridItem", "AccordionItem"] {
            let src = format!("const x = <{tag} onClick={{h}}>x</{tag}>;");
            assert_eq!(run(&src).len(), 1, "<{tag}> should be flagged");
        }
    }
}
