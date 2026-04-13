//! no-verb-in-rest-url backend — flag REST URLs that bake a verb into
//! the path instead of using HTTP semantics.
//!
//! Why: `/api/createOrder` is RPC in REST clothing. The correct form is
//! `POST /api/orders`. Verbs in URLs prevent caches from working, defeat
//! REST tooling, and create an infinite proliferation of paths
//! (`createOrder`, `updateOrder`, `cancelOrder`, `refundOrder`...).
//!
//! Detection: walk `string` nodes containing `/api/` followed by a banned
//! verb prefix in camelCase. This catches string literals used as fetch
//! URLs or route definitions.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "string" {
        return;
    }
    let Ok(text) = node.utf8_text(source) else {
        return;
    };
    let Some(verb) = super::verb_url_match::contains_verb_url(text) else {
        return;
    };
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-verb-in-rest-url".into(),
        message: format!(
            "REST URL contains the verb '{verb}' — use HTTP semantics instead. \
             `POST /api/orders` creates, `GET /api/orders/:id` reads, \
             `PATCH /api/orders/:id` updates, `DELETE /api/orders/:id` removes."
        ),
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
    fn flags_create_order_url() {
        assert_eq!(run_on("fetch('/api/createOrder');").len(), 1);
    }

    #[test]
    fn flags_delete_user_url() {
        assert_eq!(run_on("const u = '/api/deleteUser';").len(), 1);
    }

    #[test]
    fn allows_resource_url() {
        assert!(run_on("fetch('/api/orders');").is_empty());
        assert!(run_on("fetch('/api/orders/123');").is_empty());
    }

    #[test]
    fn allows_verb_in_non_api_string() {
        // Not a URL — regular string.
        assert!(run_on("const label = 'createOrder';").is_empty());
    }
}
