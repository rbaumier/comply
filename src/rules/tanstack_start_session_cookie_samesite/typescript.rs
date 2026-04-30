//! Flag `useSession({ cookie: { ... } })` when the cookie object does not
//! contain a `sameSite` key set to `'lax'` or `'strict'`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["useSession"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return; };
    let Ok(callee_text) = callee.utf8_text(source) else { return; };
    if !callee_text.ends_with("useSession") { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(options) = first_object_argument(args) else { return; };
    let Some(cookie_value) = find_pair_value(options, source, "cookie") else { return; };
    if cookie_value.kind() != "object" { return; }

    let samesite_value = find_pair_value(cookie_value, source, "sameSite")
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.trim_matches(|c| c == '"' || c == '\'').to_string());

    if samesite_value.as_deref() == Some("lax") || samesite_value.as_deref() == Some("strict") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &cookie_value,
        super::META.id,
        "`useSession` cookie config must set `sameSite` to `'lax'` or `'strict'` \
         to mitigate CSRF attacks."
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
        if child.kind() != "pair" {
            continue;
        }
        let Some(k) = child.child_by_field_name("key") else {
            continue;
        };
        let Ok(raw) = k.utf8_text(source) else {
            continue;
        };
        let name = raw.trim_matches(|c| c == '"' || c == '\'');
        if name == key {
            return child.child_by_field_name("value");
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_missing_samesite() {
        assert_eq!(
            run("useSession({ cookie: { httpOnly: true, secure: true } });").len(),
            1
        );
    }

    #[test]
    fn flags_samesite_none() {
        assert_eq!(
            run("useSession({ cookie: { sameSite: 'none' } });").len(),
            1
        );
    }

    #[test]
    fn allows_samesite_lax() {
        assert!(run("useSession({ cookie: { sameSite: 'lax' } });").is_empty());
    }

    #[test]
    fn allows_samesite_strict() {
        assert!(run("useSession({ cookie: { sameSite: 'strict' } });").is_empty());
    }
}
