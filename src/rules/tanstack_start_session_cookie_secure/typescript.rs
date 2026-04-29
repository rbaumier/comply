//! Flag `useSession({ cookie: { ... } })` when the cookie object does not
//! contain a `secure` key.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["useSession"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return; };
    let Ok(callee_text) = callee.utf8_text(source) else { return; };
    if !callee_text.ends_with("useSession") { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(options) = first_object_argument(args) else { return; };
    let Some(cookie_value) = find_pair_value(options, source, "cookie") else { return; };
    if cookie_value.kind() != "object" { return; }
    let secure_value = find_pair_value(cookie_value, source, "secure");
    if let Some(v) = secure_value {
        // Present and not literally `false` → trust the user.
        let raw = v.utf8_text(source).unwrap_or("").trim();
        if raw != "false" { return; }
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &cookie_value,
        super::META.id,
        "`useSession` cookie config must set `secure` so session cookies are \
         only transmitted over HTTPS."
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_missing_secure() {
        assert_eq!(
            run("useSession({ cookie: { httpOnly: true, sameSite: 'lax' } });").len(),
            1
        );
    }

    #[test]
    fn allows_secure_true() {
        assert!(
            run("useSession({ cookie: { secure: true, sameSite: 'lax' } });").is_empty()
        );
    }

    #[test]
    fn allows_secure_expression() {
        assert!(
            run("useSession({ cookie: { secure: isProd, sameSite: 'lax' } });").is_empty()
        );
    }

    #[test]
    fn flags_secure_false() {
        assert_eq!(
            run("useSession({ cookie: { secure: false, sameSite: 'lax' } });").len(),
            1
        );
    }
}
