//! better-auth-drizzle-useplural — require `usePlural: true` when a `users` table is used.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// Walk the object subtree and return true if `users` appears as an identifier
/// (a property key, shorthand property, or value reference) — not inside a
/// string literal or comment.
fn references_users_identifier(node: Node<'_>, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    let kind = node.kind();
    if kind == "string" || kind == "template_string" || kind == "comment" {
        return false;
    }
    if kind == "identifier" || kind == "property_identifier" {
        return node.utf8_text(source).unwrap_or("") == "users";
    }
    for child in node.children(&mut cursor) {
        if references_users_identifier(child, source) {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(func) = node.child_by_field_name("function") else { return };
    if func.utf8_text(source).unwrap_or("") != "drizzleAdapter" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(obj) = args.children(&mut cursor).find(|c| c.kind() == "object") else { return };

    // Only flag if a plural `users` identifier is referenced (not in strings/comments).
    if !references_users_identifier(obj, source) {
        return;
    }

    let obj_text = obj.utf8_text(source).unwrap_or("");
    if obj_text.contains("usePlural: true") || obj_text.contains("usePlural:true") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`drizzleAdapter` uses a plural `users` table — add `usePlural: true`.".into(),
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
    fn flags_plural_without_useplural() {
        assert_eq!(
            run("drizzleAdapter(db, { schema: { users: users } })").len(),
            1
        );
    }

    #[test]
    fn allows_with_useplural_true() {
        assert!(
            run("drizzleAdapter(db, { schema: { users: users }, usePlural: true })").is_empty()
        );
    }

    #[test]
    fn allows_singular_user() {
        assert!(run("drizzleAdapter(db, { schema: { user: user } })").is_empty());
    }

    #[test]
    fn allows_users_in_string_literal() {
        assert!(run("drizzleAdapter(db, { schema: { user: user, label: \"users\" } })").is_empty());
    }
}
