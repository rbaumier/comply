//! better-auth-require-secure-cookies — flag `betterAuth({ ... })` calls whose
//! config object doesn't mention `useSecureCookies`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.utf8_text(source).unwrap_or("") != "betterAuth" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // Arguments node has children: `(`, argument(s), `)`. Find the first object.
    let mut cursor = args.walk();
    let obj = args
        .children(&mut cursor)
        .find(|c| c.kind() == "object");
    let Some(obj) = obj else { return };

    let obj_text = obj.utf8_text(source).unwrap_or("");
    if obj_text.contains("useSecureCookies") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Better Auth config is missing `useSecureCookies: true` — add `advanced: { useSecureCookies: true }` so session cookies are only sent over HTTPS.".into(),
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
    fn flags_missing_secure_cookies() {
        assert_eq!(
            run("export const auth = betterAuth({ database: db });").len(),
            1
        );
    }

    #[test]
    fn allows_with_secure_cookies() {
        assert!(
            run("betterAuth({ advanced: { useSecureCookies: true }, database: db })").is_empty()
        );
    }

    #[test]
    fn ignores_file_without_better_auth() {
        assert!(run("const x = doSomething()").is_empty());
    }

    #[test]
    fn ignores_unrelated_call() {
        assert!(run("configure({ database: db })").is_empty());
    }
}
