//! security-no-query-without-ownership backend —
//! DB "find by id" calls without an ownership filter.

use crate::diagnostic::{Diagnostic, Severity};

fn is_find_by_id(name: &str) -> bool {
    // Match `xxx.findById`, `xxx.findOne`, `xxx.findUnique`, `xxx.findFirst`,
    // `xxx.getById`. We trigger on the short method name.
    let Some(method) = name.rsplit('.').next() else {
        return false;
    };
    matches!(
        method,
        "findById" | "findOne" | "findUnique" | "findFirst" | "getById"
    )
}

fn has_ownership_filter(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("userid")
        || lower.contains("user_id")
        || lower.contains("ownerid")
        || lower.contains("owner_id")
        || lower.contains("orgid")
        || lower.contains("org_id")
        || lower.contains("tenantid")
        || lower.contains("tenant_id")
        || lower.contains("accountid")
        || lower.contains("account_id")
}

fn path_is_script_or_internal(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    let lower = s.to_ascii_lowercase();
    for marker in [
        "/scripts/",
        "/jobs/",
        "/cron/",
        "/seed/",
        "/seeds/",
        "/admin/",
        "/migrations/",
    ] {
        if lower.contains(marker) {
            return true;
        }
    }
    false
}

fn is_in_route_handler(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    // Walk ancestors looking for either:
    // - a `.get(...) / .post(...) / .put(...) / .patch(...) / .delete(...) / .all(...)`
    //   call expression (Express/Hono/Fastify-style),
    // - an exported `GET`/`POST`/`PUT`/`PATCH`/`DELETE` function (Next.js/Remix),
    // - a function with a `req` / `request` / `ctx` parameter.
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "call_expression"
            && let Some(callee) = parent.child_by_field_name("function")
            && callee.kind() == "member_expression"
            && let Some(prop) = callee.child_by_field_name("property")
            && let Ok(name) = prop.utf8_text(source)
            && matches!(name, "get" | "post" | "put" | "patch" | "delete" | "all")
        {
            return true;
        }
        if parent.kind() == "function_declaration"
            && let Some(name) = parent.child_by_field_name("name")
            && let Ok(text) = name.utf8_text(source)
            && matches!(text, "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS")
            && let Some(grandparent) = parent.parent()
            && grandparent.kind() == "export_statement"
        {
            return true;
        }
        if matches!(
            parent.kind(),
            "function_declaration"
                | "function_expression"
                | "arrow_function"
                | "method_definition"
        ) && function_has_request_param(parent, source)
        {
            return true;
        }
        cur = parent;
    }
    false
}

fn function_has_request_param(func: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(params) = func.child_by_field_name("parameters") else { return false };
    let mut cursor = params.walk();
    for param in params.named_children(&mut cursor) {
        // param can be `required_parameter`, `identifier`, `formal_parameter`...
        let Ok(text) = param.utf8_text(source) else { continue };
        let first = text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').next().unwrap_or("");
        if matches!(first, "req" | "request" | "ctx" | "context") {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !is_find_by_id(name) {
        return;
    }

    // Skip files that look like scripts/jobs — IDOR isn't a concern there.
    if path_is_script_or_internal(ctx.path) {
        return;
    }

    // Only flag inside a route handler / API endpoint context.
    if !is_in_route_handler(node, source) {
        return;
    }

    // Scan the full call text (function + arguments) for an ownership filter.
    let Ok(full_text) = node.utf8_text(source) else {
        return;
    };
    if has_ownership_filter(full_text) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{name}` has no ownership filter (userId/orgId/tenantId) — possible IDOR."
        ),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    fn run_with_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_find_by_id_without_owner() {
        let src = "app.get('/orders/:id', (req, res) => { Order.findById(req.params.id); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_find_unique_without_owner() {
        let src = "app.get('/orders/:id', (req, res) => { prisma.order.findUnique({ where: { id: req.params.id } }); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_find_with_user_id() {
        let src = "app.get('/orders/:id', (req, res) => { prisma.order.findFirst({ where: { id: req.params.id, userId: req.user.id } }); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_find_with_org_id() {
        let src = "app.post('/orders', (req, res) => { prisma.order.findUnique({ where: { id, orgId } }); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_query_in_scripts_dir() {
        // Admin scripts/jobs aren't end-user requests; IDOR doesn't apply.
        let src = "Order.findById(someId);";
        assert!(run_with_path(src, "scripts/seed.ts").is_empty());
    }

    #[test]
    fn ignores_query_outside_route_handler() {
        // Bare top-level call with no handler context.
        let src = "Order.findById(someId);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_in_exported_get_route() {
        // Next.js / Remix-style route export.
        let src = "export async function GET(req) { return Order.findById(req.params.id); }";
        assert_eq!(run(src).len(), 1);
    }
}
