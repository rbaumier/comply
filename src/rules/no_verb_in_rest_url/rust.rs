//! no-verb-in-rest-url backend for Rust.
//!
//! Same string walk as the TypeScript version, but applied to Rust string
//! literals. Flags URLs like `/api/createOrder` / `/api/deleteUser` in
//! favor of HTTP-semantic resource paths.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const BANNED_VERBS: &[&str] = &[
    "create", "get", "update", "delete", "remove", "list", "fetch", "find",
    "add", "set", "modify", "edit", "save", "load", "cancel", "refund",
    "submit", "approve", "reject", "archive",
];

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "string_literal" && node.kind() != "raw_string_literal" {
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
                     semantics instead (POST /api/orders, GET /api/orders/:id…)."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

fn contains_verb_url(text: &str) -> Option<&'static str> {
    let inner = text.trim_matches(|c| c == '"' || c == 'r' || c == '#');
    if !inner.contains("/api/") && !inner.contains("/v1/") && !inner.contains("/v2/") {
        return None;
    }
    for &verb in BANNED_VERBS {
        let vlen = verb.len();
        let mut start = 0;
        while let Some(idx) = inner[start..].find(verb) {
            let absolute = start + idx;
            let prev = absolute.checked_sub(1).and_then(|i| inner.as_bytes().get(i));
            if prev != Some(&b'/') {
                start = absolute + vlen;
                continue;
            }
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
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.rs"), source),
            &tree,
        )
    }

    #[test]
    fn flags_create_order_url() {
        assert_eq!(run_on("fn f() { let u = \"/api/createOrder\"; }").len(), 1);
    }

    #[test]
    fn allows_resource_url() {
        assert!(run_on("fn f() { let u = \"/api/orders\"; }").is_empty());
    }
}
