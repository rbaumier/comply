//! no-verb-in-rest-url backend — flag REST URLs that bake a verb into
//! the path instead of using HTTP semantics.
//!
//! Why: `/api/createOrder` is RPC in REST clothing. The correct form is
//! `POST /api/orders`. Verbs in URLs prevent caches from working, defeat
//! REST tooling, and create an infinite proliferation of paths
//! (`createOrder`, `updateOrder`, `cancelOrder`, `refundOrder`...).
//!
//! Detection: walk string-like nodes containing `/api/` followed by a
//! banned verb prefix in camelCase. This catches literals used as fetch
//! URLs or route definitions.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["string", "template_string"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else {
        return;
    };
    let Some(verb) = super::verb_url_match::contains_verb_url(text) else {
        return;
    };
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
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

    #[test]
    fn flags_static_template_literal() {
        assert_eq!(run_on("fetch(`/api/createOrder`);").len(), 1);
    }

    #[test]
    fn flags_template_literal_route_definition() {
        assert_eq!(run_on("router.post(`/api/deleteUser`, handler);").len(), 1);
    }

    #[test]
    fn allows_dynamic_template_without_verb_segment() {
        assert!(run_on("fetch(`/api/orders/${id}`);").is_empty());
    }
}
