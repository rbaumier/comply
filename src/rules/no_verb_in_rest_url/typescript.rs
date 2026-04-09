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
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const BANNED_VERBS: &[&str] = &[
    "create", "get", "update", "delete", "remove", "list", "fetch", "find",
    "add", "set", "modify", "edit", "save", "load", "cancel", "refund",
    "submit", "approve", "reject", "archive",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "string" {
                return;
            }
            let Ok(text) = node.utf8_text(source_bytes) else {
                return;
            };
            let Some(verb) = contains_verb_url(text) else {
                return;
            };
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-verb-in-rest-url".into(),
                message: format!(
                    "REST URL contains the verb '{verb}' — use HTTP \
                     semantics instead. `POST /api/orders` creates, \
                     `GET /api/orders/:id` reads, `PATCH /api/orders/:id` \
                     updates, `DELETE /api/orders/:id` removes."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

/// Check if a string literal contains `/api/<verb><PascalCase>` or a
/// similar pattern. Returns the matched verb.
fn contains_verb_url(text: &str) -> Option<&'static str> {
    // Strip string quotes if present.
    let inner = text.trim_matches(|c| c == '"' || c == '\'' || c == '`');
    if !inner.contains("/api/") && !inner.contains("/v1/") && !inner.contains("/v2/") {
        return None;
    }
    // For each banned verb, check if it appears as the start of a path
    // segment followed by an uppercase letter (camelCase boundary).
    for &verb in BANNED_VERBS {
        let vlen = verb.len();
        let mut start = 0;
        while let Some(idx) = inner[start..].find(verb) {
            let absolute = start + idx;
            // Must be preceded by `/` (path segment start).
            let prev = absolute.checked_sub(1).and_then(|i| inner.as_bytes().get(i));
            if prev != Some(&b'/') {
                start = absolute + vlen;
                continue;
            }
            // Must be followed by an uppercase letter (camelCase).
            let next = inner.as_bytes().get(absolute + vlen);
            if next.is_some_and(|b| b.is_ascii_uppercase()) {
                return Some(verb);
            }
            start = absolute + vlen;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.ts"), source),
            &tree,
        )
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
