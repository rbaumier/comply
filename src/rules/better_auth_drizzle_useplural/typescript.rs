//! better-auth-drizzle-useplural — require `usePlural: true` when a `users` table is used.

use crate::diagnostic::{Diagnostic, Severity};

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

    let obj_text = obj.utf8_text(source).unwrap_or("");

    // Only flag if a plural `users` table is referenced.
    if !obj_text.contains("users") {
        return;
    }

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
}
