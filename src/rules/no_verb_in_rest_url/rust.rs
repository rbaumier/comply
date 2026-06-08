//! no-verb-in-rest-url backend for Rust.
//!
//! Same string walk as the TypeScript version, but applied to Rust string
//! literals. Flags URLs like `/api/createOrder` / `/api/deleteUser` in
//! favor of HTTP-semantic resource paths.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["string_literal", "raw_string_literal"] => |node, source, ctx, diagnostics|
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
            "REST URL contains the verb '{verb}' — use HTTP semantics instead \
             (POST /api/orders, GET /api/orders/:id…)."
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_create_order_url() {
        assert_eq!(run_on("fn f() { let u = \"/api/createOrder\"; }").len(), 1);
    }

    #[test]
    fn allows_resource_url() {
        assert!(run_on("fn f() { let u = \"/api/orders\"; }").is_empty());
        assert!(run_on("fn f() { let u = \"/api/orders/123\"; }").is_empty());
    }

    #[test]
    fn flags_delete_user_url() {
        assert_eq!(run_on("fn f() { let u = \"/api/deleteUser\"; }").len(), 1);
    }

    #[test]
    fn allows_verb_in_non_api_string() {
        // Not a URL — regular string.
        assert!(run_on("fn f() { let label = \"createOrder\"; }").is_empty());
    }
}
