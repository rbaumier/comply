//! AST backend for react-require-versioned-storage-key.

use crate::diagnostic::{Diagnostic, Severity};

fn is_localstorage_setitem(call: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(callee) = call.child_by_field_name("function") else { return false };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(object) = callee.child_by_field_name("object") else { return false };
    if object.kind() != "identifier" {
        return false;
    }
    if object.utf8_text(source).ok() != Some("localStorage") {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return false };
    prop.utf8_text(source).ok() == Some("setItem")
}

fn first_string_literal<'a>(
    call: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<(&'a str, tree_sitter::Node<'a>)> {
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let first = args
        .named_children(&mut cursor)
        .find(|c| c.kind() != "comment")?;
    if first.kind() != "string" {
        return None;
    }
    let raw = first.utf8_text(source).ok()?;
    let unquoted = raw.trim_matches(|c| c == '"' || c == '\'' || c == '`');
    Some((unquoted, first))
}

fn has_version_suffix(key: &str) -> bool {
    // `...:vN` where N is one or more digits.
    let Some(idx) = key.rfind(":v") else { return false };
    let suffix = &key[idx + 2..];
    !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = ctx;
    if node.kind() != "call_expression" {
        return;
    }
    if !is_localstorage_setitem(node, source) {
        return;
    }
    let Some((key, key_node)) = first_string_literal(node, source) else { return };
    if has_version_suffix(key) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &key_node,
        super::META.id,
        format!(
            "Storage key `{key}` has no `:vN` version suffix — bumping the \
             version lets you migrate or drop old entries when the shape changes."
        ),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_unversioned_key() {
        let src = r#"localStorage.setItem("settings", JSON.stringify(x));"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_versioned_key() {
        let src = r#"localStorage.setItem("settings:v1", JSON.stringify(x));"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_dynamic_key() {
        let src = r#"localStorage.setItem(key, "v");"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_versioned_v10() {
        let src = r#"localStorage.setItem("cache:v10", x);"#;
        assert!(run(src).is_empty());
    }
}
