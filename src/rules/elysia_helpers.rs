//! Shared filters for Elysia-specific rules.
//!
//! Many Elysia rules fire on syntactic shapes like `<ident>.listen(...)`
//! / `<ident>.use(...)`. In an Elysia project, the same shape also
//! appears on:
//!
//! - `msw/node`'s `setupServer().listen()` in vitest setups
//! - Express/Hono `.use(...)` middleware
//! - vanilla `http.createServer()`-style identifiers
//!
//! Without the TypeScript type checker we cannot prove the receiver is
//! an Elysia instance. The conservative heuristic below uses the
//! identifier name to keep the rules useful (catching real Elysia
//! servers named `app`, `elysia`, `*App`, `*Elysia`) while staying
//! silent on the common false-positive shapes.

use oxc_ast::ast::Expression;

/// True if `name` is a likely Elysia server-instance identifier.
///
/// Allowlisted shapes:
/// - exact `app` / `App` / `elysia` / `Elysia`
/// - any name ending in `Elysia` (`mainElysia`, `apiElysia`)
/// - any name ending in `App` (`apiApp`, `authApp`)
/// - any name starting with `elysia` (`elysiaApp`, `elysia_v2`)
///
/// **Not** allowlisted: `*Server` (catches `mswServer`,
/// `expressServer`, `httpServer`, …), `*Client`, anything else.
pub fn looks_like_elysia_identifier(name: &str) -> bool {
    matches!(name, "app" | "App" | "elysia" | "Elysia")
        || name.ends_with("Elysia")
        || name.ends_with("App")
        || name.starts_with("elysia")
}

/// Walk an expression and return the leftmost identifier name — the
/// "root" of a chain like `app.use(x).listen(...)` (returns `"app"`)
/// or `new Elysia().listen(...)` (returns `"Elysia"`).
///
/// Returns `None` for compound roots (object literals, parenthesised
/// expressions wrapping non-identifier shapes, etc.) so callers can
/// fall back to firing or silencing as they prefer.
pub fn root_identifier_name<'a>(expr: &'a Expression<'_>) -> Option<&'a str> {
    let mut current = expr;
    loop {
        match current {
            Expression::Identifier(id) => return Some(id.name.as_str()),
            Expression::CallExpression(call) => current = &call.callee,
            Expression::StaticMemberExpression(member) => current = &member.object,
            Expression::ComputedMemberExpression(member) => current = &member.object,
            Expression::ParenthesizedExpression(p) => current = &p.expression,
            Expression::NewExpression(new) => {
                return match &new.callee {
                    Expression::Identifier(id) => Some(id.name.as_str()),
                    _ => None,
                };
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlists_canonical_names() {
        assert!(looks_like_elysia_identifier("app"));
        assert!(looks_like_elysia_identifier("App"));
        assert!(looks_like_elysia_identifier("elysia"));
        assert!(looks_like_elysia_identifier("Elysia"));
    }

    #[test]
    fn allowlists_suffix_app_and_elysia() {
        assert!(looks_like_elysia_identifier("apiApp"));
        assert!(looks_like_elysia_identifier("authApp"));
        assert!(looks_like_elysia_identifier("mainElysia"));
    }

    #[test]
    fn rejects_msw_and_other_server_names() {
        // Regression for rbaumier/comply#21 — MSW's setupServer().listen()
        // and similar non-Elysia *Server identifiers must not trigger.
        assert!(!looks_like_elysia_identifier("mswServer"));
        assert!(!looks_like_elysia_identifier("expressServer"));
        assert!(!looks_like_elysia_identifier("httpServer"));
        assert!(!looks_like_elysia_identifier("server"));
    }

    #[test]
    fn rejects_unrelated_identifiers() {
        assert!(!looks_like_elysia_identifier("router"));
        assert!(!looks_like_elysia_identifier("client"));
        assert!(!looks_like_elysia_identifier("vitest"));
    }
}
