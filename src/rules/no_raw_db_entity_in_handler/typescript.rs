use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "delete", "patch"];

const DB_PATTERNS: &[&str] = &[
    "prisma", "db", "knex",
];

const DB_METHODS: &[&str] = &[
    "findMany", "findFirst", "findUnique", "query",
];

/// Returns true if this call expression looks like a DB call
/// (e.g. `prisma.user.findMany()`, `knex(...)`, `db.query(...)`).
fn is_db_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    let text = node.utf8_text(source).unwrap_or("");
    // Direct calls like `knex("items")`
    if let Some(callee) = node.child_by_field_name("function")
        && callee.kind() == "identifier" {
            let name = callee.utf8_text(source).unwrap_or("");
            if DB_PATTERNS.contains(&name) {
                return true;
            }
        }
    // Member calls: check if any DB pattern + method combination is present
    for pat in DB_PATTERNS {
        for method in DB_METHODS {
            if text.contains(pat) && text.contains(method) {
                return true;
            }
        }
    }
    false
}

/// Returns true if node is (or is inside) a route handler callback.
fn is_route_handler(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "call_expression"
            && let Some(callee) = n.child_by_field_name("function")
                && callee.kind() == "member_expression"
                    && let Some(prop) = callee.child_by_field_name("property") {
                        let method = prop.utf8_text(source).unwrap_or("");
                        if ROUTE_METHODS.contains(&method) {
                            return true;
                        }
                    }
        current = n.parent();
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    if !is_db_call(node, source) {
        return;
    }

    if !is_route_handler(node, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-raw-db-entity-in-handler".into(),
        message: "Direct DB call in route handler — map to a DTO before returning.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_prisma_in_handler() {
        let src = r#"
app.get("/users", async (c) => {
    const users = await prisma.user.findMany();
    return c.json(users);
});
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_handler_without_db() {
        let src = r#"
app.get("/health", (c) => {
    return c.json({ ok: true });
});
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_knex_in_route() {
        let src = r#"
router.post("/items", async (c) => {
    const items = await knex("items").select("*");
    return c.json(items);
});
"#;
        assert_eq!(run(src).len(), 1);
    }
}
