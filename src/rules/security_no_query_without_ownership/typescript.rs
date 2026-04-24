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

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !is_find_by_id(name) {
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

    #[test]
    fn flags_find_by_id_without_owner() {
        assert_eq!(run("Order.findById(req.params.id);").len(), 1);
    }

    #[test]
    fn flags_find_unique_without_owner() {
        let src = "prisma.order.findUnique({ where: { id: req.params.id } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_find_with_user_id() {
        let src = "prisma.order.findFirst({ where: { id: req.params.id, userId: req.user.id } });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_find_with_org_id() {
        let src = "prisma.order.findUnique({ where: { id, orgId } });";
        assert!(run(src).is_empty());
    }
}
