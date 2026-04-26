//! better-auth-reset-password-handler — require `sendResetPassword` when `emailAndPassword.enabled`.

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

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key
        .utf8_text(source)
        .unwrap_or("")
        .trim_matches(|c: char| c == '\'' || c == '"');
    if key_text != "emailAndPassword" {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "object" {
        return;
    }

    let Some(enabled) = find_pair_with_key(value, source, "enabled") else { return };
    let Some(enabled_val) = enabled.child_by_field_name("value") else { return };
    if enabled_val.utf8_text(source).unwrap_or("").trim() != "true" {
        return;
    }

    if find_pair_with_key(value, source, "sendResetPassword").is_some() {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`emailAndPassword.enabled: true` requires a `sendResetPassword` handler.".into(),
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
        let src = "betterAuth({ emailAndPassword: { enabled: true } })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_handler() {
        let src = "betterAuth({ emailAndPassword: { enabled: true, sendResetPassword: async (x) => {} } })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_when_disabled() {
        let src = "betterAuth({ emailAndPassword: { enabled: false } })";
        assert!(run(src).is_empty());
    }
}
