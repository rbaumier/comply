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
//!
//! A parameter is also left alone when the body moves it by value into a
//! struct or enum-variant literal (`Thing { name }` / `Variant { error }` /
//! `Thing { name: name }`). There the function genuinely needs ownership,
//! so taking `String` is the correct API — switching to `&str` would only
//! shift the allocation into the body. A borrow (`&name`) or a clone
//! (`name.clone()`) does not consume the owned value and still warrants the
//! warning.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_in_test_context;

crate::ast_check! { on ["function_item"] => |node, source, ctx, diagnostics|
    if is_in_test_context(node, source) { return; }
    if !is_pub(node, source) { return; }

    let Some(params) = node.child_by_field_name("parameters") else { return; };
    let body = node.child_by_field_name("body");
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

        // Skip params the body moves by value into a struct/enum literal —
        // ownership is genuinely needed there.
        let Some(pattern) = param.child_by_field_name("pattern") else { continue; };
        if pattern.kind() != "identifier" { continue; }
        let Ok(param_name) = pattern.utf8_text(source) else { continue; };
        if let Some(body) = body
            && param_moved_into_struct(body, source, param_name)
        {
            continue;
        }

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

/// Whether `param_name` is moved by value into a struct or enum-variant
/// literal anywhere in `node`'s subtree. A field value that is a bare
/// `identifier` equal to the param (`{ x }` shorthand, or `{ x: x }`)
/// consumes the owned value; `&x` (`reference_expression`) or `x.clone()`
/// (`call_expression`) do not and are ignored.
fn param_moved_into_struct(node: tree_sitter::Node, source: &[u8], param_name: &str) -> bool {
    if node.kind() == "struct_expression"
        && let Some(fields) = node.child_by_field_name("body")
    {
        let mut cursor = fields.walk();
        for field in fields.named_children(&mut cursor) {
            let value = match field.kind() {
                "shorthand_field_initializer" => {
                    let mut field_cursor = field.walk();
                    field
                        .named_children(&mut field_cursor)
                        .find(|c| c.kind() == "identifier")
                }
                "field_initializer" => field.child_by_field_name("value"),
                _ => None,
            };
            if let Some(value) = value
                && value.kind() == "identifier"
                && value.utf8_text(source) == Ok(param_name)
            {
                return true;
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if param_moved_into_struct(child, source, param_name) {
            return true;
        }
    }
    false
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_pub_fn_with_owned_string() {
        assert_eq!(
            run("pub fn greet(name: String) -> String { name }").len(),
            1
        );
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
        assert!(run("pub fn greet(name: Cow<'_, str>) -> String { name.into_owned() }").is_empty());
    }

    #[test]
    fn allows_mut_string_param() {
        assert!(run("pub fn fill(mut buf: String) -> String { buf.push('x'); buf }").is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        assert!(run("#[cfg(test)]\nmod tests { pub fn f(s: String) {} }").is_empty());
    }

    #[test]
    fn allows_param_moved_into_enum_variant() {
        assert!(
            run("pub fn volt_installing(&self, error: String) { self.notification(CoreNotification::VoltInstalling { error }); }")
                .is_empty()
        );
    }

    #[test]
    fn allows_param_moved_into_struct_shorthand() {
        assert!(run("fn make(name: String) -> Thing { Thing { name } }").is_empty());
    }

    #[test]
    fn allows_param_moved_into_struct_explicit_field() {
        assert!(run("pub fn make(name: String) -> Thing { Thing { name: name } }").is_empty());
    }

    #[test]
    fn flags_param_only_read_in_body() {
        assert_eq!(
            run(r#"pub fn log(msg: String) { println!("{}", msg); }"#).len(),
            1
        );
    }

    #[test]
    fn flags_param_read_only_len() {
        assert_eq!(run("pub fn count(s: String) -> usize { s.len() }").len(), 1);
    }

    #[test]
    fn flags_param_borrowed_into_struct() {
        assert_eq!(
            run("pub fn make(name: String) -> Thing { Thing { name: &name } }").len(),
            1
        );
    }

    #[test]
    fn flags_param_cloned_into_struct() {
        assert_eq!(
            run("pub fn make(name: String) -> Thing { Thing { name: name.clone() } }").len(),
            1
        );
    }
}
