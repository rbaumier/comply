//! rust-impl-debug-on-public-types backend.
//!
//! For every `struct_item` and `enum_item` with a strictly `pub` visibility
//! modifier, scan the preceding `attribute_item` siblings looking
//! for either `#[derive(...Debug...)]` or a manual `impl Debug for
//! ...` somewhere in the file. Flag if neither is present.
//!
//! Suppressed for: `pub(crate)`/`pub(super)`/`pub(in …)` visibility,
//! files under `tests/` or `benches/`, items in a `#[cfg(test)]` module,
//! items gated on `#[cfg(doctest)]` (the README-doctest harness — compiled only
//! when rustdoc collects doctests, never reachable by consumers),
//! items in a `proc-macro = true` crate (whose `pub` types are unreachable by
//! consumers), items with `#[doc(hidden)]`, items carrying
//! `#[allow(missing_debug_implementations)]` or
//! `#[expect(missing_debug_implementations)]` (the rustc lint this rule
//! mirrors — the author has explicitly opted out), types with
//! raw-pointer fields, and types that store a closure/function in a field
//! whose generic type parameter carries an `Fn`/`FnMut`/`FnOnce` bound (the
//! combinator pattern in poem/tower/axum — closures don't implement `Debug`,
//! so neither a derive nor a `Debug`-bounded field is viable).
//!
//! We accept manual impls because libraries with closure or PhantomData
//! fields legitimately can't derive — they hand-roll the impl. The manual
//! impl is matched on the AST trait/target of every `impl_item` in the file,
//! so generic impls (`impl<T: Debug> Debug for Wrapper<'_, T>`) and any
//! trait-path spelling (`Debug`, `fmt::Debug`, `std::fmt::Debug`,
//! `core::fmt::Debug`) are recognized.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{has_doc_hidden, has_test_attribute, is_in_test_context};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["struct_item", "enum_item"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let kind = node.kind();
        if !is_pub(node, source_bytes) {
            return;
        }
        if ctx.path.components().any(|c| {
            c.as_os_str() == "tests" || c.as_os_str() == "benches"
        }) {
            return;
        }
        if is_in_test_context(node, source_bytes)
            || has_test_attribute(node, source_bytes)
            || has_cfg_doctest_attr(node, source_bytes)
        {
            return;
        }
        // A `proc-macro = true` crate can export only procedural macros; its
        // `pub` types are unreachable by any consumer, so "consumers can't
        // debug it" is structurally inapplicable.
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.is_proc_macro())
        {
            return;
        }
        if has_doc_hidden(node, source_bytes) {
            return;
        }
        if has_allow_missing_debug(node, source_bytes) {
            return;
        }
        if has_raw_pointer_field(node) {
            return;
        }
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Ok(name) = name_node.utf8_text(source_bytes) else {
            return;
        };
        // A field holding a closure/function (its type is a generic param with
        // an `Fn`/`FnMut`/`FnOnce` bound) can't derive `Debug`: closures don't
        // implement it, and the bound usually lives on an `impl` block, not the
        // struct itself — so this scans both. Same "can't derive" class as a
        // raw-pointer field.
        if holds_closure_typed_field(node, name, source_bytes) {
            return;
        }
        if has_debug_derive(node, source_bytes) {
            return;
        }
        // A hand-written `Debug` impl for this type anywhere in the file.
        if has_manual_debug_impl(node, name, source_bytes) {
            return;
        }
        let pos = node.start_position();
        let kind_label = if kind == "struct_item" {
            "struct"
        } else {
            "enum"
        };
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-impl-debug-on-public-types".into(),
            message: format!(
                "`pub {kind_label} {name}` has no `Debug` impl — \
                 consumers can't log it, can't use it in assert \
                 failure messages, can't see it in `{{:?}}` output. \
                 Add `#[derive(Debug)]` or implement `Debug` by hand."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_pub(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = item.walk();
    for child in item.children(&mut cursor) {
        if child.kind() == "visibility_modifier"
            && let Ok(text) = child.utf8_text(source)
            && text == "pub"
        {
            return true;
        }
    }
    false
}

fn has_debug_derive(item: tree_sitter::Node, source: &[u8]) -> bool {
    // Walk every preceding sibling; keep going through attribute_item
    // and comment nodes (both `line_comment` and `block_comment`, which
    // tree-sitter-rust inserts between attributes when a trailing `//`
    // or block comment sits beside an attribute like
    // `#[allow(...)] // trailing note`). Stop at the first sibling that
    // isn't an attribute or a comment — that's where our declaration's
    // attribute block actually ends.
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                if let Ok(text) = s.utf8_text(source)
                    && text.contains("derive(")
                    && text.contains("Debug")
                {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {
                // Comments interleaved with attributes don't end the
                // attribute block. Keep walking.
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True when the file contains a hand-written `Debug` impl for `name`.
///
/// Walks every `impl_item` in the file from the `source_file` root. An
/// `impl_item` is a *trait* impl only when it has a `trait` field (inherent
/// impls have none and are skipped). The trait matches `Debug` when its final
/// `::` segment is `Debug`, covering `Debug`, `fmt::Debug`, `std::fmt::Debug`,
/// `core::fmt::Debug`, and any other `*::Debug` path. The target matches when
/// its base type identifier — stripped of generic arguments, lifetimes, and a
/// leading path — equals `name` exactly. The `impl<...>` generic-parameter
/// clause is a separate node that does not affect the `trait`/`type` fields, so
/// generic impls (`impl<T: Debug> Debug for Wrapper<'_, T>`) match naturally.
fn has_manual_debug_impl(node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "impl_item"
            && let Some(trait_node) = n.child_by_field_name("trait")
            && let Ok(trait_text) = trait_node.utf8_text(source)
            && trait_text.rsplit("::").next() == Some("Debug")
            && let Some(target_node) = n.child_by_field_name("type")
            && base_type_name(target_node, source) == Some(name)
        {
            return true;
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// The base type identifier of an `impl` target, ignoring generic arguments,
/// lifetimes, and a leading module path. `Wrapper<'_, T>` (`generic_type`) →
/// `Wrapper`; `Closure` (`type_identifier`) → `Closure`; `crate::Span`
/// (`scoped_type_identifier`) → `Span`. Returns `None` for shapes that have no
/// single base name (references, tuples, etc.).
fn base_type_name<'a>(target: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    match target.kind() {
        // `Wrapper<'_, T>` — the base name lives in the `type` field.
        "generic_type" => base_type_name(target.child_by_field_name("type")?, source),
        "type_identifier" => target.utf8_text(source).ok(),
        // `std::fmt::Foo` — the final `::` segment is the base name.
        "scoped_type_identifier" => target.utf8_text(source).ok().and_then(|t| t.rsplit("::").next()),
        _ => None,
    }
}

/// True if a preceding `attribute_item` sibling is
/// `#[allow(missing_debug_implementations)]` or
/// `#[expect(missing_debug_implementations)]`. That is the exact rustc lint
/// this rule mirrors, so an explicit allow/expect of it means the author has
/// deliberately opted out and we defer to that.
///
/// Walks preceding siblings like `has_doc_hidden`/`has_debug_derive`, skipping
/// interleaved comment siblings. The match is specific: the attribute path must
/// be `allow` or `expect` AND its argument list must contain
/// `missing_debug_implementations`, so an unrelated `#[allow(dead_code)]` does
/// not suppress.
fn has_allow_missing_debug(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                if attribute_allows_lint(s, source, "missing_debug_implementations") {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if `attribute_item` is an `allow`/`expect` attribute whose argument list
/// names `lint`, bare or tool-scoped (`rustc::<lint>`).
///
/// `attribute_item` parses as `attribute_item > attribute`, where the
/// `attribute` is `seq($._path, optional(arguments: token_tree))`: its first
/// named child is the path (`allow`/`expect`) and its arguments live in the
/// `token_tree` as a flat sequence of `identifier` tokens. We match on the AST
/// path child (`allow`/`expect`) and on the `identifier` tokens inside the
/// token tree rather than scanning raw text, so a lint merely ending in
/// `_missing_debug_implementations`, or the name appearing inside an unrelated
/// string, does not match. A tool-scoped `rustc::missing_debug_implementations`
/// still tokenizes the final segment as its own `identifier`, so it matches too.
fn attribute_allows_lint(attribute_item: tree_sitter::Node, source: &[u8], lint: &str) -> bool {
    let mut item_cursor = attribute_item.walk();
    let Some(attribute) = attribute_item
        .children(&mut item_cursor)
        .find(|child| child.kind() == "attribute")
    else {
        return false;
    };

    let Some(path) = attribute.named_child(0) else {
        return false;
    };
    let Ok(path_text) = path.utf8_text(source) else {
        return false;
    };
    if path_text != "allow" && path_text != "expect" {
        return false;
    }

    let Some(token_tree) = attribute.child_by_field_name("arguments") else {
        return false;
    };

    let mut tree_cursor = token_tree.walk();
    token_tree
        .children(&mut tree_cursor)
        .any(|tok| tok.kind() == "identifier" && tok.utf8_text(source) == Ok(lint))
}

/// True if a preceding `attribute_item` sibling is `#[cfg(doctest)]`.
///
/// A `#[cfg(doctest)]` item is compiled only when rustdoc collects doctests —
/// the README-doctest harness (`#[doc = include_str!("../README.md")]
/// #[cfg(doctest)] pub struct ReadmeDoctests;`) — so it never exists in normal
/// builds and is unreachable by consumers. It is the same class of build-gated,
/// non-API item as `#[cfg(test)]`.
///
/// Walks preceding siblings like `has_allow_missing_debug`, skipping interleaved
/// comment siblings; the `#[doc = include_str!(...)]` of the harness is itself
/// an `attribute_item` sibling that the walk traverses. The match is specific:
/// the attribute path must be `cfg` AND `doctest` must appear as a bare
/// `identifier` token directly inside the `cfg(...)` token tree. So
/// `#[cfg(feature = "x")]` does not match (no bare `doctest` identifier) and
/// `#[cfg(not(doctest))]` does not match (its `doctest` is nested inside the
/// `not(...)` token tree, not a direct child of the `cfg` token tree).
fn has_cfg_doctest_attr(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                if attribute_is_cfg_doctest(s, source) {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if `attribute_item` is `#[cfg(doctest)]`: a `cfg` attribute whose
/// `token_tree` arguments contain `doctest` as a direct-child `identifier`
/// token. Mirrors the AST traversal in `attribute_allows_lint` — match on the
/// path child (`cfg`) and on the `identifier` tokens inside the token tree
/// rather than scanning raw text. Matching `doctest` only as a *direct* child of
/// the `cfg` token tree excludes `#[cfg(not(doctest))]`, whose `doctest` lives
/// inside a nested `not(...)` token tree.
fn attribute_is_cfg_doctest(attribute_item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut item_cursor = attribute_item.walk();
    let Some(attribute) = attribute_item
        .children(&mut item_cursor)
        .find(|child| child.kind() == "attribute")
    else {
        return false;
    };

    let Some(path) = attribute.named_child(0) else {
        return false;
    };
    if path.utf8_text(source) != Ok("cfg") {
        return false;
    }

    let Some(token_tree) = attribute.child_by_field_name("arguments") else {
        return false;
    };

    let mut tree_cursor = token_tree.walk();
    token_tree
        .children(&mut tree_cursor)
        .any(|tok| tok.kind() == "identifier" && tok.utf8_text(source) == Ok("doctest"))
}

fn has_raw_pointer_field(item: tree_sitter::Node) -> bool {
    let mut cursor = item.walk();
    loop {
        if cursor.node().kind() == "pointer_type" {
            return true;
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() || cursor.node().id() == item.id() {
                return false;
            }
        }
    }
}

/// True when `struct_node` stores a field whose type is one of the struct's own
/// generic type parameters, and that parameter carries an `Fn`/`FnMut`/`FnOnce`
/// bound somewhere reachable. Such a field holds a closure/function, which never
/// implements `Debug`, so the type genuinely can't derive it.
///
/// The closure bound is searched in two places, because the combinator pattern
/// (`pub struct Map<E, F> { inner: E, f: F }` with the `Fn` bound on a separate
/// `impl ... for Map<E, F> where F: Fn(...) -> ...` block) puts it on the impl,
/// not the struct:
///   a. the struct's own inline type-parameter bounds and struct-level
///      `where_clause`, and
///   b. every `impl_item` in the file whose target base type equals `name`
///      (its inline impl-parameter bounds and `where_clause`).
///
/// In tree-sitter-rust, `F: Fn(R) -> Fut` is a `where_predicate`
/// (`left: type_identifier` `F`, `bounds: trait_bounds`) and the `Fn(R) -> Fut`
/// itself is a `function_type` whose `trait` field is `Fn`/`FnMut`/`FnOnce`
/// (bare `type_identifier` or the final segment of a `scoped_type_identifier`
/// like `std::ops::Fn`). The same `function_type` shape appears in inline
/// type-parameter bounds (`F: Fn() -> i32`).
fn holds_closure_typed_field(struct_node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    let field_param_names = field_typed_generic_params(struct_node, source);
    if field_param_names.is_empty() {
        return false;
    }

    // (a) The struct's own generic-parameter bounds and struct-level where clause.
    if let Some(type_params) = struct_node.child_by_field_name("type_parameters")
        && type_parameters_bind_closure(type_params, &field_param_names, source)
    {
        return true;
    }
    if let Some(where_clause) = child_of_kind(struct_node, "where_clause")
        && where_clause_binds_closure(where_clause, &field_param_names, source)
    {
        return true;
    }

    // (b) Every `impl ... for <name>` block in the file.
    let mut root = struct_node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "impl_item"
            && let Some(target) = n.child_by_field_name("type")
            && base_type_name(target, source) == Some(name)
        {
            if let Some(type_params) = n.child_by_field_name("type_parameters")
                && type_parameters_bind_closure(type_params, &field_param_names, source)
            {
                return true;
            }
            if let Some(where_clause) = child_of_kind(n, "where_clause")
                && where_clause_binds_closure(where_clause, &field_param_names, source)
            {
                return true;
            }
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// The struct's declared generic type parameters that are used directly as the
/// type of a field — i.e. a `field_declaration` whose `type` is a bare
/// `type_identifier` equal to a declared parameter name (e.g. `f: F`). Only
/// these can be closure-typed, so they are the candidates we test for an `Fn`
/// bound. Lifetimes and const generics are skipped (only `type_identifier`
/// parameters are collected).
fn field_typed_generic_params<'a>(struct_node: tree_sitter::Node, source: &'a [u8]) -> Vec<&'a str> {
    let declared = declared_type_param_names(struct_node, source);
    if declared.is_empty() {
        return Vec::new();
    }
    let Some(body) = struct_node.child_by_field_name("body") else {
        return Vec::new();
    };
    let mut cursor = body.walk();
    let mut params = Vec::new();
    for field in body.children(&mut cursor) {
        if field.kind() != "field_declaration" {
            continue;
        }
        if let Some(ty) = field.child_by_field_name("type")
            && ty.kind() == "type_identifier"
            && let Ok(text) = ty.utf8_text(source)
            && declared.contains(&text)
            && !params.contains(&text)
        {
            params.push(text);
        }
    }
    params
}

/// Names of the `type_identifier` generic parameters declared on the struct's
/// `type_parameters` node (skipping lifetimes and const generics).
fn declared_type_param_names<'a>(struct_node: tree_sitter::Node, source: &'a [u8]) -> Vec<&'a str> {
    let Some(type_params) = struct_node.child_by_field_name("type_parameters") else {
        return Vec::new();
    };
    let mut cursor = type_params.walk();
    let mut names = Vec::new();
    for param in type_params.children(&mut cursor) {
        if param.kind() != "type_parameter" {
            continue;
        }
        if let Some(name_node) = param.child_by_field_name("name")
            && name_node.kind() == "type_identifier"
            && let Ok(text) = name_node.utf8_text(source)
        {
            names.push(text);
        }
    }
    names
}

/// True if any inline `type_parameter` bound binds one of `field_params` to an
/// `Fn`/`FnMut`/`FnOnce` trait (`<F: Fn() -> i32>`). A `type_parameter`'s
/// `name` is the bound left-hand side, and its `bounds: trait_bounds` holds the
/// `function_type` nodes.
fn type_parameters_bind_closure(
    type_params: tree_sitter::Node,
    field_params: &[&str],
    source: &[u8],
) -> bool {
    let mut cursor = type_params.walk();
    for param in type_params.children(&mut cursor) {
        if param.kind() != "type_parameter" {
            continue;
        }
        let Some(name_node) = param.child_by_field_name("name") else {
            continue;
        };
        let Ok(lhs) = name_node.utf8_text(source) else {
            continue;
        };
        if !field_params.contains(&lhs) {
            continue;
        }
        if let Some(bounds) = param.child_by_field_name("bounds")
            && trait_bounds_have_closure(bounds, source)
        {
            return true;
        }
    }
    false
}

/// True if any `where_predicate` in `where_clause` constrains one of
/// `field_params` to an `Fn`/`FnMut`/`FnOnce` trait. A `where_predicate` has a
/// `left` type and a `bounds: trait_bounds`; the predicate matches only when its
/// `left` is a bare `type_identifier` equal to a field parameter.
fn where_clause_binds_closure(
    where_clause: tree_sitter::Node,
    field_params: &[&str],
    source: &[u8],
) -> bool {
    let mut cursor = where_clause.walk();
    for predicate in where_clause.children(&mut cursor) {
        if predicate.kind() != "where_predicate" {
            continue;
        }
        let Some(left) = predicate.child_by_field_name("left") else {
            continue;
        };
        if left.kind() != "type_identifier" {
            continue;
        }
        let Ok(lhs) = left.utf8_text(source) else {
            continue;
        };
        if !field_params.contains(&lhs) {
            continue;
        }
        if let Some(bounds) = predicate.child_by_field_name("bounds")
            && trait_bounds_have_closure(bounds, source)
        {
            return true;
        }
    }
    false
}

/// True if a `trait_bounds` list contains a `function_type` bound whose trait is
/// `Fn`/`FnMut`/`FnOnce`. `Fn(R) -> Fut`, `FnMut()`, `FnOnce()` each parse as a
/// `function_type` (not a `trait_bound`), with a `trait` field that is a bare
/// `type_identifier` or a `scoped_type_identifier` (`std::ops::Fn`) whose final
/// segment is the trait name.
fn trait_bounds_have_closure(bounds: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = bounds.walk();
    bounds.children(&mut cursor).any(|bound| {
        bound.kind() == "function_type"
            && bound
                .child_by_field_name("trait")
                .and_then(|t| base_type_name(t, source))
                .is_some_and(is_closure_trait)
    })
}

fn is_closure_trait(name: &str) -> bool {
    matches!(name, "Fn" | "FnMut" | "FnOnce")
}

/// First direct child of `node` of the given `kind` (used for `where_clause`,
/// which is a non-field named child of both `struct_item` and `impl_item`).
fn child_of_kind<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).find(|c| c.kind() == kind)
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

    fn run_with_path(source: &str, fake_path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, fake_path)
    }

    /// Run on a file in `dir/src/x.rs` next to the given `Cargo.toml`, so
    /// `nearest_cargo_manifest` resolves the temp crate's manifest (e.g. for
    /// the `proc-macro = true` exemption).
    fn run_on_with_cargo(cargo_toml_contents: &str, source: &str) -> Vec<Diagnostic> {
        use std::fs;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml_contents).unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        let src_path = dir.path().join("src/x.rs");
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
    }

    #[test]
    fn flags_pub_struct_without_debug() {
        assert_eq!(run_on("pub struct User { name: String }").len(), 1);
    }

    #[test]
    fn flags_pub_enum_without_debug() {
        assert_eq!(run_on("pub enum State { Idle, Busy }").len(), 1);
    }

    #[test]
    fn allows_pub_struct_with_debug_derive() {
        assert!(run_on("#[derive(Debug)]\npub struct User { name: String }").is_empty());
    }

    #[test]
    fn allows_pub_struct_with_mixed_derive() {
        assert!(
            run_on("#[derive(Clone, Debug, Default)]\npub struct User { name: String }").is_empty()
        );
    }

    #[test]
    fn allows_pub_struct_with_manual_debug_impl() {
        let source = "pub struct Closure { f: Box<dyn Fn()> }\nimpl Debug for Closure { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_private_struct() {
        assert!(run_on("struct User { name: String }").is_empty());
    }

    #[test]
    fn allows_doc_comment_above_multi_attribute_block() {
        // Reproduces the RuleMeta false positive: a doc comment, then
        // `#[derive(Debug, ...)]`, then another `#[allow(...)]`, then
        // the struct. The walker must traverse both attribute items
        // without being stopped by the preceding doc comment.
        let source = "/// Doc line 1.\n\
                      /// Doc line 2.\n\
                      #[derive(Debug, Clone, Copy)]\n\
                      #[allow(dead_code)]\n\
                      pub struct RuleMeta { pub id: &'static str }";
        assert!(
            run_on(source).is_empty(),
            "false positive: multi-attribute block with Debug derive should not fire"
        );
    }

    #[test]
    fn suppresses_pub_crate_struct() {
        assert!(run_on("pub(crate) struct Internal { x: u8 }").is_empty());
    }

    #[test]
    fn suppresses_pub_struct_in_tests_dir() {
        assert!(run_with_path("pub struct X;", "tests/foo.rs").is_empty());
    }

    #[test]
    fn suppresses_pub_struct_in_benches_dir() {
        assert!(run_with_path("pub struct X;", "benches/bench.rs").is_empty());
    }

    #[test]
    fn suppresses_doc_hidden_enum() {
        assert!(run_on("#[doc(hidden)]\npub enum Y {}").is_empty());
    }

    #[test]
    fn suppresses_cfg_test_struct() {
        assert!(run_on("#[cfg(test)]\npub struct Z;").is_empty());
    }

    #[test]
    fn suppresses_struct_inside_cfg_test_mod() {
        assert!(run_on("#[cfg(test)]\nmod tests {\n    pub struct TestHelper;\n}").is_empty());
    }

    #[test]
    fn suppresses_raw_pointer_field() {
        assert!(run_on("pub struct W { p: *const u8 }").is_empty());
    }

    /// Closes #3834: the standard README-doctest harness — a unit struct
    /// carrying `#[doc = include_str!("../README.md")]` gated on
    /// `#[cfg(doctest)]` — is compiled only when rustdoc collects doctests, so
    /// it is never reachable by consumers and requiring `Debug` is meaningless.
    /// It is the same class of build-gated, non-API item as `#[cfg(test)]`.
    #[test]
    fn suppresses_cfg_doctest_readme_harness() {
        let source = "#[doc = include_str!(\"../README.md\")]\n\
                      #[cfg(doctest)]\n\
                      pub struct ReadmeDoctests;";
        assert!(
            run_on(source).is_empty(),
            "the #[cfg(doctest)] README-doctest harness struct must not be flagged"
        );
    }

    /// A non-doctest `cfg` gate (`#[cfg(feature = "x")]`) leaves the type in the
    /// public API of normal builds — the exemption is doctest-specific, so it
    /// must still flag.
    #[test]
    fn still_flags_cfg_feature_gated_struct() {
        assert_eq!(
            run_on("#[cfg(feature = \"x\")]\npub struct Bar { name: String }").len(),
            1,
            "a #[cfg(feature = \"x\")] gate must not exempt; the exemption is doctest-specific"
        );
    }

    /// `#[cfg(not(doctest))]` is production-only (compiled when rustdoc is *not*
    /// collecting doctests), so it must still flag — the exemption triggers only
    /// on a positive `doctest` predicate.
    #[test]
    fn still_flags_cfg_not_doctest_struct() {
        assert_eq!(
            run_on("#[cfg(not(doctest))]\npub struct Bar { name: String }").len(),
            1,
            "a #[cfg(not(doctest))] gate is production-only and must still flag"
        );
    }

    #[test]
    fn still_flags_plain_pub_struct() {
        assert_eq!(run_on("pub struct Api { name: String }").len(), 1);
    }

    #[test]
    fn suppresses_allow_missing_debug_implementations() {
        // tokio net/addr.rs:269-270 — author explicitly opted out of the
        // rustc lint this rule mirrors.
        assert!(
            run_on("#[allow(missing_debug_implementations)]\npub struct Internal;").is_empty()
        );
    }

    #[test]
    fn suppresses_expect_missing_debug_implementations() {
        assert!(run_on("#[expect(missing_debug_implementations)]\npub struct X;").is_empty());
    }

    #[test]
    fn suppresses_allow_missing_debug_with_interleaved_cfg_attr() {
        // tokio runtime/task_hooks.rs:57-59 — a `#[cfg_attr(...)]` sits
        // between the allow and the item; the walk must traverse it.
        let source = "#[allow(missing_debug_implementations)]\n\
                      #[cfg_attr(not(tokio_unstable), allow(unreachable_pub))]\n\
                      pub struct TaskMeta<'a> { id: &'a str }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn suppresses_tool_scoped_allow_missing_debug() {
        // rustc accepts a tool-scoped lint path; the final segment still
        // tokenizes as a bare identifier inside the token tree.
        assert!(
            run_on("#[allow(rustc::missing_debug_implementations)]\npub struct X;").is_empty()
        );
    }

    #[test]
    fn still_flags_with_unrelated_allow() {
        // `#[allow(dead_code)]` is unrelated; suppression is lint-specific.
        assert_eq!(run_on("#[allow(dead_code)]\npub struct X { name: String }").len(), 1);
    }

    const PROC_MACRO_CARGO_TOML: &str = r#"
[package]
name = "prost-derive-like"
version = "0.1.0"
edition = "2021"

[lib]
proc-macro = true
"#;

    const LIB_CARGO_TOML: &str = r#"
[package]
name = "normal-lib"
version = "0.1.0"
edition = "2021"

[lib]
name = "normal_lib"
"#;

    /// Closes #3960: a `pub` type in a `proc-macro = true` crate is unreachable
    /// by any consumer (prost-derive `field/scalar.rs:585` etc.), so it must
    /// not be flagged.
    #[test]
    fn suppresses_pub_type_in_proc_macro_crate() {
        assert!(
            run_on_with_cargo(PROC_MACRO_CARGO_TOML, "pub enum Kind { Bool, Int }").is_empty(),
            "must not flag pub types in a proc-macro crate"
        );
        assert!(
            run_on_with_cargo(PROC_MACRO_CARGO_TOML, "pub struct Field { name: String }")
                .is_empty(),
            "must not flag pub structs in a proc-macro crate"
        );
    }

    /// A normal library crate (`[lib]` without `proc-macro = true`) exposes its
    /// `pub` types to consumers — the exemption is proc-macro-only, so it must
    /// still flag.
    #[test]
    fn still_flags_pub_type_in_normal_lib_crate() {
        assert_eq!(
            run_on_with_cargo(LIB_CARGO_TOML, "pub struct Api { name: String }").len(),
            1,
            "a normal lib crate's pub type with no Debug must still flag"
        );
    }

    #[test]
    fn allows_manual_debug_impl_with_std_path() {
        let source = "pub struct X { x: u32 }\nimpl std::fmt::Debug for X { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_manual_debug_impl_with_fmt_path() {
        let source = "pub struct X { x: u32 }\nimpl fmt::Debug for X { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) } }";
        assert!(run_on(source).is_empty());
    }

    /// Closes #3904: `no_std` crates (regex-syntax) spell the manual impl with
    /// the `core::fmt::Debug` path; a non-generic struct with that impl must not
    /// be flagged.
    #[test]
    fn allows_manual_debug_impl_with_core_path() {
        let source = "pub struct Span { x: u32 }\nimpl core::fmt::Debug for Span { fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result { Ok(()) } }";
        assert!(
            run_on(source).is_empty(),
            "core::fmt::Debug manual impl (no_std) must be recognized"
        );
    }

    /// Closes #3738: a generic manual impl (`impl<T: Debug> Debug for
    /// Wrapper<'_, T>`) must be recognized — the `impl<...>` prefix and the
    /// generic args on the target previously defeated the literal-string match.
    #[test]
    fn allows_generic_manual_debug_impl() {
        let source = "pub struct Wrapper<T>(T);\nimpl<T: Debug> Debug for Wrapper<T> { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) } }";
        assert!(
            run_on(source).is_empty(),
            "generic manual Debug impl must be recognized"
        );
    }

    /// A generic manual impl with lifetimes and a scoped trait path, matching
    /// the oxc_allocator `impl<T: ?Sized + Debug> fmt::Debug for Box<'_, T>`
    /// shape from #3738.
    #[test]
    fn allows_generic_manual_debug_impl_with_lifetime_and_scoped_trait() {
        let source = "pub struct Boxed<'a, T>(&'a T);\nimpl<T: ?Sized + Debug> fmt::Debug for Boxed<'_, T> { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) } }";
        assert!(run_on(source).is_empty());
    }

    /// A `Debug` impl for a *different* type must not exempt this one — the
    /// base-type match is exact, not a substring (`SpanOther` ≠ `Span`).
    #[test]
    fn still_flags_when_debug_impl_targets_different_type() {
        let source = "pub struct Span { x: u32 }\nimpl core::fmt::Debug for SpanOther { fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result { Ok(()) } }";
        assert_eq!(
            run_on(source).len(),
            1,
            "a Debug impl for SpanOther must not exempt Span"
        );
    }

    /// A manual impl of a *different* trait (`Display`) is not a `Debug` impl —
    /// the type still has no `Debug` and must be flagged.
    #[test]
    fn still_flags_when_only_non_debug_trait_impl_exists() {
        let source = "pub struct Span { x: u32 }\nimpl Display for Span { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) } }";
        assert_eq!(
            run_on(source).len(),
            1,
            "a Display impl does not count as a Debug impl"
        );
    }

    /// No derive and no manual impl at all — the type must still be flagged.
    #[test]
    fn still_flags_when_no_debug_impl_at_all() {
        assert_eq!(run_on("pub struct NoDebug { x: u32 }").len(), 1);
    }

    /// Closes #4440 (issue repro): a poem-style combinator struct holds a
    /// closure in `f: F`, with the `Fn` bound on the `impl` block's `where`
    /// clause (not the struct). Closures don't implement `Debug`, so the type
    /// can't derive it and must not be flagged.
    #[test]
    fn suppresses_combinator_with_fn_bound_on_impl() {
        let source = "pub struct Map<E, F> {\n    inner: E,\n    f: F,\n}\n\
                      impl<E, F, Fut, R, R2> Endpoint for Map<E, F>\n\
                      where\n    F: Fn(R) -> Fut + Send + Sync,\n{\n}";
        assert!(
            run_on(source).is_empty(),
            "a combinator struct with an `Fn`-bound closure field must not be flagged"
        );
    }

    /// The `FnMut`/`FnOnce` variants of the impl-`where` closure bound are the
    /// same case and must also be suppressed.
    #[test]
    fn suppresses_combinator_with_fnmut_and_fnonce_bound_on_impl() {
        let fnmut = "pub struct After<E, F> {\n    inner: E,\n    f: F,\n}\n\
                     impl<E, F> Endpoint for After<E, F>\n\
                     where\n    F: FnMut() -> i32,\n{\n}";
        assert!(
            run_on(fnmut).is_empty(),
            "an `FnMut`-bound closure field must not be flagged"
        );
        let fnonce = "pub struct AndThen<E, F> {\n    inner: E,\n    f: F,\n}\n\
                      impl<E, F> Endpoint for AndThen<E, F>\n\
                      where\n    F: FnOnce() -> i32,\n{\n}";
        assert!(
            run_on(fnonce).is_empty(),
            "an `FnOnce`-bound closure field must not be flagged"
        );
    }

    /// The bound can also sit on the struct itself (`<F: Fn() -> i32>`), as an
    /// inline type-parameter bound — that case is suppressed too.
    #[test]
    fn suppresses_struct_level_inline_fn_bound() {
        assert!(
            run_on("pub struct S<F: Fn() -> i32> { f: F }").is_empty(),
            "an inline struct-level `Fn` bound on the field's param must not be flagged"
        );
    }

    /// Load-bearing negative: a plain generic data wrapper with NO `Fn` bound
    /// anywhere can and should derive `Debug` — the closure guard must not
    /// over-suppress it.
    #[test]
    fn still_flags_plain_generic_wrapper() {
        assert_eq!(
            run_on("pub struct Wrapper<T> { value: T }").len(),
            1,
            "a plain generic data wrapper (no Fn bound) must still be flagged"
        );
    }

    /// Load-bearing negative: a plain non-generic struct is unaffected by the
    /// closure guard and must still be flagged.
    #[test]
    fn still_flags_plain_non_generic_struct() {
        assert_eq!(
            run_on("pub struct Foo { x: i32 }").len(),
            1,
            "a plain non-generic struct must still be flagged"
        );
    }

    #[test]
    fn allows_trailing_comment_after_inner_attribute() {
        // Reproduces the exact RuleMeta shape in meta.rs: a trailing
        // `// comment` after `#[allow(dead_code)]` between the derive
        // and the struct. tree-sitter-rust may split this differently.
        let source = "/// Doc.\n\
                      #[derive(Debug, Clone, Copy)]\n\
                      #[allow(dead_code)] // Fields read by JSON output / explain / remap (coming soon).\n\
                      pub struct RuleMeta { pub id: &'static str }";
        assert!(
            run_on(source).is_empty(),
            "false positive: trailing line comment after attribute should not defeat Debug detection"
        );
    }
}
