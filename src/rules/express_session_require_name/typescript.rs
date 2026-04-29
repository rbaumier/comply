//! express-session-require-name — flag `session({ ... })` calls whose
//! config object is missing the `name` property. Relying on the default
//! cookie name (`connect.sid`) leaks the server stack and lets attackers
//! fingerprint session cookies.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["session"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "identifier" {
        return;
    }
    if func.utf8_text(source).unwrap_or("") != "session" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let obj = args
        .children(&mut cursor)
        .find(|c| c.kind() == "object");
    let Some(obj) = obj else { return };

    if object_has_property(&obj, source, "name") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "session config is missing `name` — add a custom cookie name so the default `connect.sid` doesn't leak the server stack.".into(),
        Severity::Warning,
    ));
}

fn object_has_property(obj: &tree_sitter::Node, source: &[u8], key: &str) -> bool {
    let mut cursor = obj.walk();
    for child in obj.children(&mut cursor) {
        if child.kind() != "pair" && child.kind() != "shorthand_property_identifier" {
            continue;
        }
        let key_node = match child.kind() {
            "pair" => child.child_by_field_name("key"),
            _ => Some(child),
        };
        let Some(k) = key_node else { continue };
        let text = k.utf8_text(source).unwrap_or("");
        // Strip surrounding quotes for string keys.
        let text = text.trim_matches(|c| c == '"' || c == '\'' || c == '`');
        if text == key {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_missing_name() {
        assert_eq!(
            run("app.use(session({ secret: 'keyboard cat', resave: false }));").len(),
            1
        );
    }

    #[test]
    fn allows_with_name() {
        assert!(
            run("app.use(session({ name: 'sid', secret: 'keyboard cat', resave: false }));")
                .is_empty()
        );
    }

    #[test]
    fn allows_with_quoted_name_key() {
        assert!(run("session({ 'name': 'sid', secret: 's' })").is_empty());
    }

    #[test]
    fn ignores_session_without_args() {
        assert!(run("session()").is_empty());
    }

    #[test]
    fn ignores_unrelated_call() {
        assert!(run("configure({ secret: 's' })").is_empty());
    }

    #[test]
    fn ignores_non_object_argument() {
        assert!(run("session(opts)").is_empty());
    }
}
