//! better-auth-required-user-fields — require `email` and `name` in the `user` config.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// Recursively scan the object subtree for a property whose key matches `name`.
/// Considers property keys (`pair`) and shorthand identifiers (`shorthand_property_identifier`).
fn has_property_key(node: Node<'_>, source: &[u8], name: &str) -> bool {
    let kind = node.kind();
    if kind == "pair"
        && let Some(key) = node.child_by_field_name("key")
    {
        let key_text = key
            .utf8_text(source)
            .unwrap_or("")
            .trim_matches(|c: char| c == '\'' || c == '"');
        if key_text == name {
            return true;
        }
    }
    if kind == "shorthand_property_identifier" && node.utf8_text(source).unwrap_or("") == name {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_property_key(child, source, name) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
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

    let has_email = has_property_key(value, source, "email");
    let has_name = has_property_key(value, source, "name");

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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
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

    #[test]
    fn flags_when_email_and_name_only_in_strings() {
        let src = "betterAuth({ user: { additionalFields: { role: { type: 'string', label: 'email and name' } } } })";
        assert_eq!(run(src).len(), 1);
    }
}
