//! api-first AST backend — flag files that register an HTTP route
//! (e.g. `app.get(...)`, `router.post(...)`) without referencing any
//! schema validator.
//!
//! Fires once per file at the `program` root, then walks the AST to:
//!   1. Find the first `call_expression` whose callee is a `member_expression`
//!      with property in {get, post, put, delete} — the route registration.
//!   2. Search the AST for any schema indicator: an identifier named `z`,
//!      `createRoute`, `openapi`, `schema`, or `zodValidator`.
//!
//! If a route exists and no schema indicator is found, emit a diagnostic
//! anchored on the route call.

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "delete"];
const SCHEMA_INDICATORS: &[&str] = &["z", "createRoute", "openapi", "schema", "zodValidator"];

/// Walk `root` and return the first `call_expression` node whose callee
/// is `<recv>.<method>` with method in [`ROUTE_METHODS`].
fn find_route_call<'a>(
    root: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut stack = vec![root];
    let mut found: Option<tree_sitter::Node<'a>> = None;
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression"
            && let Some(func) = n.child_by_field_name("function")
            && func.kind() == "member_expression"
            && let Some(prop) = func.child_by_field_name("property")
        {
            let name = std::str::from_utf8(&source[prop.byte_range()]).unwrap_or("");
            if ROUTE_METHODS.contains(&name) {
                // Pick the earliest in source order.
                let start = n.start_byte();
                if found.map(|f| start < f.start_byte()).unwrap_or(true) {
                    found = Some(n);
                }
            }
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    found
}

/// Walk `root` looking for any identifier matching one of
/// [`SCHEMA_INDICATORS`]. Tree-sitter exposes references as either
/// `identifier`, `property_identifier`, or `type_identifier`.
fn has_schema_indicator(root: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let kind = n.kind();
        if kind == "identifier" || kind == "property_identifier" || kind == "type_identifier" {
            let name = std::str::from_utf8(&source[n.byte_range()]).unwrap_or("");
            if SCHEMA_INDICATORS.contains(&name) {
                return true;
            }
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    // Fire once per file.
    let Some(route) = find_route_call(node, source) else { return };
    if has_schema_indicator(node, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &route,
        super::META.id,
        "Route handler without schema definition — define the API schema (e.g. `z.object`, `zodValidator`) before the handler.".into(),
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
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_route_without_schema() {
        let src = r#"
app.get("/users", (c) => {
    return c.json([]);
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_route_with_zod_schema() {
        let src = r#"
const querySchema = z.object({ page: z.number() });
app.get("/users", zodValidator("query", querySchema), (c) => {
    return c.json([]);
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_route_file() {
        let src = r#"
export function getUsers() {
    return db.query("SELECT * FROM users");
}
"#;
        assert!(run_on(src).is_empty());
    }
}
