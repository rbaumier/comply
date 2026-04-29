//! elysia-better-auth-null-session backend — flag missing null-session check around `getSession`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["program"] prefilter = ["auth.api.getSession"] => |node, source, ctx, diagnostics|
    let _ = (node, source);
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !ctx.source.contains("auth.api.getSession") {
        return;
    }
    if !ctx.source.contains("resolve") {
        return;
    }

    if ctx.source.contains("status(401")
        || ctx.source.contains("!session")
        || ctx.source.contains("session === null")
        || ctx.source.contains("session == null")
    {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: 1,
        column: 1,
        rule_id: "elysia-better-auth-null-session".into(),
        message: "Better Auth `getSession` can return null — add `if (!session) return status(401)` before using it.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_resolve_without_null_check() {
        let src = "import { auth } from 'better-auth';\nnew Elysia().macro('user', { resolve: async ({ request }) => { const session = await auth.api.getSession({ headers: request.headers }); return { user: session.user, session }; } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_resolve_with_null_check() {
        let src = "import { auth } from 'better-auth';\nnew Elysia().macro('user', { resolve: async ({ request, status }) => { const session = await auth.api.getSession({ headers: request.headers }); if (!session) return status(401); return { user: session.user, session }; } });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_files_without_better_auth() {
        let src = "const session = await getSession(); return { user: session.user, session };";
        assert!(run_on(src).is_empty());
    }
}
