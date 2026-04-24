//! Flag `useSession({ cookie: { ... } })` when the cookie object does not
//! contain `httpOnly: true`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    let Ok(callee_text) = callee.utf8_text(source) else { return; };
    if !callee_text.ends_with("useSession") { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(options) = first_object_argument(args) else { return; };
    let Some(cookie_value) = find_pair_value(options, source, "cookie") else { return; };
    if cookie_value.kind() != "object" { return; }
    if has_pair_with_value(cookie_value, source, "httpOnly", "true") { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &cookie_value,
        super::META.id,
        "`useSession` cookie config must set `httpOnly: true` to prevent \
         JavaScript access to the session cookie."
            .into(),
        Severity::Warning,
    ));
}

fn first_object_argument<'a>(args: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = args.walk();
    args.children(&mut cursor).find(|c| c.kind() == "object")
}

fn find_pair_value<'a>(
    object: tree_sitter::Node<'a>,
    source: &[u8],
    key: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = object.walk();
    for child in object.children(&mut cursor) {
        if child.kind() != "pair" { continue; }
        let Some(k) = child.child_by_field_name("key") else { continue; };
        let Ok(raw) = k.utf8_text(source) else { continue; };
        let name = raw.trim_matches(|c| c == '"' || c == '\'');
        if name == key {
            return child.child_by_field_name("value");
        }
    }
    None
}

fn has_pair_with_value(object: tree_sitter::Node<'_>, source: &[u8], key: &str, val: &str) -> bool {
    find_pair_value(object, source, key)
        .and_then(|n| n.utf8_text(source).ok())
        .is_some_and(|v| v == val)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_missing_httponly() {
        assert_eq!(
            run("useSession({ password: env.SECRET, cookie: { secure: true } });").len(),
            1
        );
    }

    #[test]
    fn flags_httponly_false() {
        assert_eq!(
            run("useSession({ cookie: { httpOnly: false, secure: true } });").len(),
            1
        );
    }

    #[test]
    fn allows_httponly_true() {
        assert!(
            run("useSession({ cookie: { httpOnly: true, secure: true } });").is_empty()
        );
    }
}
