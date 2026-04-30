//! rust-no-unwrap-in-from-impl backend.
//!
//! Walks `impl_item` nodes whose `trait` field starts with `From`
//! (so `impl From<X> for Y` and `impl<T> From<X<T>> for Y<T>`) and
//! scans the impl body for `.unwrap()` / `.expect()` method calls.
//! `TryFrom` impls are not flagged — there, fallibility is part of
//! the contract.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["impl_item"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        // The trait being implemented sits in the `trait` field.
        // For `impl From<X> for Y`, the field's text starts with `From`.
        // We must NOT match `TryFrom` — same prefix, different contract.
        let Some(trait_node) = node.child_by_field_name("trait") else {
            return;
        };
        let Ok(trait_text) = trait_node.utf8_text(source_bytes) else {
            return;
        };
        if !is_from_impl(trait_text) {
            return;
        }
        // Walk the impl body looking for `.unwrap()` / `.expect()`.
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        collect_unwraps_in(body, source_bytes, ctx, diagnostics);
    }
}

/// True if the trait reference is `From<...>` (NOT `TryFrom<...>`).
fn is_from_impl(text: &str) -> bool {
    let trimmed = text.trim_start();
    if trimmed.starts_with("TryFrom") {
        return false;
    }
    trimmed.starts_with("From")
}

fn collect_unwraps_in(
    body: tree_sitter::Node,
    source: &[u8],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression"
            && let Some(function) = node.child_by_field_name("function")
            && function.kind() == "field_expression"
            && let Some(field) = function.child_by_field_name("field")
            && let Ok(field_text) = field.utf8_text(source)
            && (field_text == "unwrap" || field_text == "expect")
        {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-no-unwrap-in-from-impl".into(),
                message: format!(
                    "`.{field_text}()` inside a `From` impl breaks the \
                     infallibility contract. Switch the impl to `TryFrom` \
                     so callers can handle the failure mode."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_unwrap_in_from_impl() {
        let source = "impl From<&str> for u32 { fn from(s: &str) -> Self { s.parse().unwrap() } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_expect_in_from_impl() {
        let source = r#"impl From<String> for Url {
            fn from(s: String) -> Self { Url::parse(&s).expect("bad url") }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_unwrap_in_try_from_impl() {
        let source = r#"impl TryFrom<&str> for u32 {
            type Error = ParseIntError;
            fn try_from(s: &str) -> Result<Self, Self::Error> { Ok(s.parse().unwrap()) }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_clean_from_impl() {
        let source = "impl From<u32> for u64 { fn from(x: u32) -> Self { x as u64 } }";
        assert!(run_on(source).is_empty());
    }
}
