//! auth-on-mutation AST backend — mutation route handlers (POST/PUT/DELETE/PATCH)
//! should reference auth.

use crate::diagnostic::{Diagnostic, Severity};

const MUTATION_METHODS: &[&str] = &["post", "put", "delete", "patch"];
const AUTH_KEYWORDS: &[&str] = &[
    "auth", "token", "session", "middleware", "guard", "protect", "verify",
];

/// Check whether a subtree contains any auth-related identifier (case-insensitive).
fn has_auth_reference(node: tree_sitter::Node, source: &[u8]) -> bool {
    let text = node.utf8_text(source).unwrap_or("");
    let lower = text.to_lowercase();
    AUTH_KEYWORDS.iter().any(|k| lower.contains(k))
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Match `app.post(`, `app.put(`, `app.delete(`, `app.patch(`
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }
    let Some(prop) = func.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");
    if !MUTATION_METHODS.contains(&method) {
        return;
    }

    // Check the full call expression text for auth keywords.
    if has_auth_reference(node, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "auth-on-mutation".into(),
        message: "Mutation route without auth check — add authentication/authorization.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_post_without_auth() {
        let src = r#"
app.post("/users", async (c) => {
    const body = await c.req.json();
    return c.json({ ok: true });
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_post_with_auth_middleware() {
        let src = r#"
app.post("/users", authMiddleware, async (c) => {
    const body = await c.req.json();
    return c.json({ ok: true });
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_delete_with_verify() {
        let src = r#"
app.delete("/users/:id", async (c) => {
    const verified = verifyToken(c);
    return c.json({ ok: true });
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_put_without_auth() {
        let src = r#"
app.put("/users/:id", async (c) => {
    const body = await c.req.json();
    return c.json({ ok: true });
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_get_without_auth() {
        let src = r#"
app.get("/users", async (c) => {
    return c.json({ users: [] });
});
"#;
        assert!(run_on(src).is_empty());
    }
}
