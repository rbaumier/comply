//! no-verb-in-rest-url backend for Rust.
//!
//! Same string walk as the TypeScript version, but applied to Rust string
//! literals. Flags URLs like `/api/createOrder` / `/api/deleteUser` in
//! favor of HTTP-semantic resource paths.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "string_literal" && node.kind() != "raw_string_literal" {
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
            "REST URL contains the verb '{verb}' — use HTTP semantics instead \
             (POST /api/orders, GET /api/orders/:id…)."
        ),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


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
