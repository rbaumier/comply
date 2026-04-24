//! better-auth-email-verification-handler — require `sendVerificationEmail` when `sendOnSignUp: true`.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

fn find_pair_with_key<'a>(obj: Node<'a>, source: &[u8], key: &str) -> Option<Node<'a>> {
    let mut cursor = obj.walk();
    for child in obj.children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let Some(k) = child.child_by_field_name("key") else {
            continue;
        };
        let k_text = k
            .utf8_text(source)
            .unwrap_or("")
            .trim_matches(|c: char| c == '\'' || c == '"');
        if k_text == key {
            return Some(child);
        }
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "pair" {
        return;
    }

    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key
        .utf8_text(source)
        .unwrap_or("")
        .trim_matches(|c: char| c == '\'' || c == '"');
    if key_text != "emailVerification" {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "object" {
        return;
    }

    let Some(send_on_signup) = find_pair_with_key(value, source, "sendOnSignUp") else { return };
    let Some(sos_val) = send_on_signup.child_by_field_name("value") else { return };
    if sos_val.utf8_text(source).unwrap_or("").trim() != "true" {
        return;
    }

    if find_pair_with_key(value, source, "sendVerificationEmail").is_some() {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`sendOnSignUp: true` requires a `sendVerificationEmail` handler.".into(),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_missing_handler() {
        let src = "betterAuth({ emailVerification: { sendOnSignUp: true } })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_handler() {
        let src = "betterAuth({ emailVerification: { sendOnSignUp: true, sendVerificationEmail: async (u) => {} } })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_when_disabled() {
        let src = "betterAuth({ emailVerification: { sendOnSignUp: false } })";
        assert!(run(src).is_empty());
    }
}
