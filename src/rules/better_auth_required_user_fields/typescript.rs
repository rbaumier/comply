//! better-auth-required-user-fields — require `email` and `name` in the `user` config.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "pair" {
        return;
    }

    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key
        .utf8_text(source)
        .unwrap_or("")
        .trim_matches(|c: char| c == '\'' || c == '"');
    if key_text != "user" {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "object" {
        return;
    }

    let obj_text = value.utf8_text(source).unwrap_or("");

    let has_email = obj_text.contains("email");
    let has_name = obj_text.contains("name");

    if has_email && has_name {
        return;
    }

    let missing = match (has_email, has_name) {
        (false, false) => "`email` and `name`",
        (false, true) => "`email`",
        (true, false) => "`name`",
        _ => return,
    };

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("`user` schema is missing {missing} — both fields are required."),
        Severity::Warning,
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
    fn flags_missing_both() {
        let src = "betterAuth({ user: { additionalFields: { role: { type: 'string' } } } })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_missing_name() {
        let src = "betterAuth({ user: { additionalFields: { email: { type: 'string' } } } })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_email_and_name() {
        let src = "betterAuth({ user: { additionalFields: { email: {}, name: {} } } })";
        assert!(run(src).is_empty());
    }
}
