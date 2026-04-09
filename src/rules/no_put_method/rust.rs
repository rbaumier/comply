//! no-put-method backend for Rust.
//!
//! Flags `client.put(...)` calls on HTTP clients and `Method::PUT`
//! constants. In Rust the equivalent of JS `fetch(..., { method: 'PUT' })`
//! is typically `reqwest::Client::put(url)` or `http::Method::PUT`. PUT
//! is "replace the entire resource" — almost every partial update wants
//! PATCH instead.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            // Flag `Method::PUT` as a scoped_identifier.
            if node.kind() == "scoped_identifier"
                && node.utf8_text(source_bytes).is_ok_and(|t| t.ends_with("Method::PUT"))
            {
                push(ctx, node, &mut diagnostics);
                return;
            }
            // Flag `.put(` method calls on HTTP clients.
            if node.kind() == "call_expression"
                && let Some(function) = node.child_by_field_name("function")
                && function.kind() == "field_expression"
                && let Some(field) = function.child_by_field_name("field")
                && field.utf8_text(source_bytes).is_ok_and(|t| t == "put")
                && let Some(receiver) = function.child_by_field_name("value")
                && looks_like_http_client(receiver, source_bytes)
            {
                push(ctx, node, &mut diagnostics);
            }
        });
        diagnostics
    }
}

/// Heuristic: the receiver is a value whose text contains `client`,
/// `request`, or `reqwest` — distinguishes HTTP clients from arbitrary
/// types that happen to have a `.put()` method.
fn looks_like_http_client(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else {
        return false;
    };
    let lower = text.to_ascii_lowercase();
    lower.contains("client") || lower.contains("reqwest") || lower.contains("request")
}

fn push(ctx: &CheckCtx, node: tree_sitter::Node, diagnostics: &mut Vec<Diagnostic>) {
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-put-method".into(),
        message: "PUT replaces the entire resource — most update-style \
                  endpoints want PATCH (partial update). Use `client.patch()` \
                  or `Method::PATCH` unless you genuinely need full replacement."
            .into(),
        severity: Severity::Warning,
    });
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
    fn flags_method_put_constant() {
        assert_eq!(run_on("fn f() { let m = Method::PUT; }").len(), 1);
    }

    #[test]
    fn flags_client_put_call() {
        assert_eq!(run_on("fn f() { client.put(url).send(); }").len(), 1);
    }

    #[test]
    fn allows_client_patch() {
        assert!(run_on("fn f() { client.patch(url).send(); }").is_empty());
    }

    #[test]
    fn does_not_flag_non_http_put() {
        // `map.put(k, v)` on a non-client receiver shouldn't fire.
        assert!(run_on("fn f() { map.put(k, v); }").is_empty());
    }
}
