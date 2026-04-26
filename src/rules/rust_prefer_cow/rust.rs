//! rust-prefer-cow backend.
//!
//! Walks `function_item` nodes, keeps those with a `pub` visibility
//! modifier (callers outside the crate are the ones that suffer from a
//! forced `.to_string()` at the call site). For each `parameter` child
//! of the signature, flag when:
//!
//! - the `type` field text is exactly `String`, and
//! - the parameter pattern is not `mut` (mutable ownership is a real
//!   signal the function rewrites the buffer; leave those alone).
//!
//! Generic `String` aliases (`std::string::String`), `&String`, and
//! `Option<String>` are deliberately not flagged — keeping the match
//! shallow mirrors the other conservative Rust rules in this crate.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_in_test_context;

crate::ast_check! { on ["function_item"] => |node, source, ctx, diagnostics|
    if is_in_test_context(node, source) { return; }
    if !is_pub(node, source) { return; }

    let Some(params) = node.child_by_field_name("parameters") else { return; };
    let mut cursor = params.walk();
    for param in params.named_children(&mut cursor) {
        if param.kind() != "parameter" { continue; }
        let Some(type_node) = param.child_by_field_name("type") else { continue; };
        let Ok(type_text) = type_node.utf8_text(source) else { continue; };
        if type_text.trim() != "String" { continue; }
        // Skip `mut s: String` — author explicitly wants ownership to mutate in place.
        // tree-sitter-rust exposes `mut` on a parameter as an anonymous
        // `mutable_specifier` child, not via the `pattern` field.
        let mut param_cursor = param.walk();
        let has_mut = param.children(&mut param_cursor)
            .any(|c| c.kind() == "mutable_specifier");
        if has_mut { continue; }

        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &param,
            super::META.id,
            "Public fn takes owned `String` — forces every caller to allocate. Prefer `&str` (no ownership) or `impl Into<Cow<'_, str>>` (conditional ownership).".into(),
            Severity::Warning,
        ));
    }
}

fn is_pub(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = item.walk();
    for child in item.children(&mut cursor) {
        if child.kind() == "visibility_modifier"
            && let Ok(text) = child.utf8_text(source)
            && text.starts_with("pub")
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_pub_fn_with_owned_string() {
        assert_eq!(run("pub fn greet(name: String) -> String { name }").len(), 1);
    }

    #[test]
    fn flags_pub_fn_with_two_string_params() {
        assert_eq!(
            run("pub fn join(a: String, b: String) -> String { a + &b }").len(),
            2
        );
    }

    #[test]
    fn allows_private_fn_with_owned_string() {
        assert!(run("fn greet(name: String) -> String { name }").is_empty());
    }

    #[test]
    fn allows_string_slice_param() {
        assert!(run("pub fn greet(name: &str) -> String { name.into() }").is_empty());
    }

    #[test]
    fn allows_string_ref_param() {
        assert!(run("pub fn greet(name: &String) -> String { name.clone() }").is_empty());
    }

    #[test]
    fn allows_cow_param() {
        assert!(
            run("pub fn greet(name: Cow<'_, str>) -> String { name.into_owned() }").is_empty()
        );
    }

    #[test]
    fn allows_mut_string_param() {
        assert!(run("pub fn fill(mut buf: String) -> String { buf.push('x'); buf }").is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        assert!(run("#[cfg(test)]\nmod tests { pub fn f(s: String) {} }").is_empty());
    }
}
