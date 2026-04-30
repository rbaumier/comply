//! Require `<SheetTitle>` inside every `<SheetContent>`.

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
    if !tag_matches(tag, "SheetContent", ".Content") {
        return;
    }
    if tag.contains('.') && !tag.starts_with("Sheet.") {
        return;
    }

    if kind == "jsx_self_closing_element"
        || !has_descendant_with_tag(node, source, "SheetTitle", ".Title")
    {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`<SheetContent>` is missing `<SheetTitle>` — required for screen readers.".into(),
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
    fn flags_sheet_content_without_title() {
        assert_eq!(
            run(r#"const x = <SheetContent><p>hi</p></SheetContent>;"#).len(),
            1
        );
    }

    #[test]
    fn flags_self_closing_sheet_content() {
        assert_eq!(run(r#"const x = <SheetContent />;"#).len(), 1);
    }

    #[test]
    fn allows_sheet_content_with_title() {
        assert!(
            run(r#"const x = <SheetContent><SheetTitle>Hi</SheetTitle></SheetContent>;"#)
                .is_empty()
        );
    }

    #[test]
    fn ignores_dialog_content() {
        assert!(run(r#"const x = <DialogContent>hi</DialogContent>;"#).is_empty());
    }
}
