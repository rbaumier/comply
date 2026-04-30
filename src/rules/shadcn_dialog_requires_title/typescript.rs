//! Walk `<DialogContent>` `jsx_element` nodes and require a
//! `<DialogTitle>` somewhere in the subtree. A self-closing
//! `<DialogContent />` trivially fails the rule.
//!
//! The `Dialog.Content` / `Dialog.Title` dotted variant is accepted too.

use crate::diagnostic::{Diagnostic, Severity};

fn tag_matches(tag: &str, flat: &str, dotted_suffix: &str) -> bool {
    tag == flat || tag.ends_with(dotted_suffix)
}

fn has_descendant_with_tag(
    node: tree_sitter::Node,
    source: &[u8],
    flat: &str,
    dotted_suffix: &str,
) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();
        if (kind == "jsx_opening_element" || kind == "jsx_self_closing_element")
            && let Some(tag) = crate::rules::jsx::jsx_element_tag_name(child, source)
            && tag_matches(tag, flat, dotted_suffix)
        {
            return true;
        }
        if has_descendant_with_tag(child, source, flat, dotted_suffix) {
            return true;
        }
    }
    false
}

fn opening_tag_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "jsx_element" => {
            let open = node.child_by_field_name("open_tag")?;
            crate::rules::jsx::jsx_element_tag_name(open, source)
        }
        "jsx_self_closing_element" => crate::rules::jsx::jsx_element_tag_name(node, source),
        _ => None,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "jsx_element" && kind != "jsx_self_closing_element" {
        return;
    }
    let Some(tag) = opening_tag_name(node, source) else {
        return;
    };
    if !tag_matches(tag, "DialogContent", ".Content") {
        return;
    }
    // `.Content` also matches e.g. `Popover.Content`; restrict to `Dialog.Content`.
    if tag.contains('.') && !tag.starts_with("Dialog.") {
        return;
    }

    if kind == "jsx_self_closing_element"
        || !has_descendant_with_tag(node, source, "DialogTitle", ".Title")
    {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`<DialogContent>` is missing `<DialogTitle>` — required for screen readers.".into(),
            Severity::Error,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_dialog_content_without_title() {
        assert_eq!(
            run(r#"const x = <DialogContent><p>hi</p></DialogContent>;"#).len(),
            1
        );
    }

    #[test]
    fn flags_self_closing_dialog_content() {
        assert_eq!(run(r#"const x = <DialogContent />;"#).len(), 1);
    }

    #[test]
    fn allows_dialog_content_with_title() {
        assert!(
            run(r#"const x = <DialogContent><DialogTitle>Hi</DialogTitle></DialogContent>;"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_dotted_dialog_content_with_title() {
        assert!(
            run(r#"const x = <Dialog.Content><Dialog.Title>Hi</Dialog.Title></Dialog.Content>;"#)
                .is_empty()
        );
    }

    #[test]
    fn ignores_popover_content() {
        assert!(run(r#"const x = <Popover.Content>hi</Popover.Content>;"#).is_empty());
    }
}
