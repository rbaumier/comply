//! Require `<AvatarFallback>` inside every `<Avatar>`.
//!
//! Matches the flat form (`Avatar`, `AvatarFallback`) and the dotted
//! form (`Avatar.Root`, `Avatar.Fallback`). We only target the Avatar
//! root, not the inner `AvatarImage` / `AvatarFallback` components
//! themselves.

use crate::diagnostic::{Diagnostic, Severity};

fn is_avatar_root(tag: &str) -> bool {
    tag == "Avatar" || tag == "Avatar.Root"
}

fn has_avatar_fallback(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();
        if (kind == "jsx_opening_element" || kind == "jsx_self_closing_element")
            && let Some(tag) = crate::rules::jsx::jsx_element_tag_name(child, source)
            && (tag == "AvatarFallback" || tag == "Avatar.Fallback")
        {
            return true;
        }
        if has_avatar_fallback(child, source) {
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

crate::ast_check! { prefilter = ["Avatar"] => |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "jsx_element" && kind != "jsx_self_closing_element" {
        return;
    }
    let Some(tag) = opening_tag_name(node, source) else {
        return;
    };
    if !is_avatar_root(tag) {
        return;
    }

    if kind == "jsx_self_closing_element" || !has_avatar_fallback(node, source) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`<Avatar>` is missing `<AvatarFallback>` — add one so broken images still render gracefully.".into(),
            Severity::Warning,
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
    fn flags_avatar_without_fallback() {
        assert_eq!(
            run(r#"const x = <Avatar><AvatarImage src="/a.png" /></Avatar>;"#).len(),
            1
        );
    }

    #[test]
    fn flags_self_closing_avatar() {
        assert_eq!(run(r#"const x = <Avatar />;"#).len(), 1);
    }

    #[test]
    fn allows_avatar_with_fallback() {
        assert!(run(r#"const x = <Avatar><AvatarImage src="/a.png" /><AvatarFallback>AB</AvatarFallback></Avatar>;"#).is_empty());
    }

    #[test]
    fn allows_dotted_avatar_with_fallback() {
        assert!(
            run(r#"const x = <Avatar.Root><Avatar.Fallback>AB</Avatar.Fallback></Avatar.Root>;"#)
                .is_empty()
        );
    }
}
