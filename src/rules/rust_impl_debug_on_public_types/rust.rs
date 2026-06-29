//! rust-impl-debug-on-public-types backend.
//!
//! For every `struct_item` and `enum_item` that is effectively public — a bare
//! `pub` modifier with no enclosing non-public module — scan the preceding
//! `attribute_item` siblings looking for either `#[derive(...Debug...)]` or a
//! manual `impl Debug for ...` somewhere in the file. Flag if neither is present.
//!
//! Suppressed for: `pub(crate)`/`pub(super)`/`pub(in …)` visibility,
//! items confined to a non-public enclosing module — whether an inline
//! `mod priv { pub struct … }` or a split file declared `mod foo;` (non-`pub`)
//! in its parent, since effective visibility never escapes the crate —
//! files under `tests/` or `benches/`, items in a `#[cfg(test)]` module,
//! items gated on `#[cfg(doctest)]` (the README-doctest harness — compiled only
//! when rustdoc collects doctests, never reachable by consumers),
//! items in a `proc-macro = true` crate (whose `pub` types are unreachable by
//! consumers), items in a binary-only crate (no `[lib]` target and no
//! `src/lib.rs`, so `pub` is merely crate-internal module visibility and no
//! external consumer can import the type), items with `#[doc(hidden)]`, items
//! covered by `#[allow(missing_debug_implementations)]` /
//! `#[expect(missing_debug_implementations)]` — the rustc lint this rule
//! mirrors — whether spelled as an item-level outer attribute, an outer
//! attribute on an enclosing `mod`, or a file/module-level inner attribute
//! `#![allow(missing_debug_implementations)]` (which suppresses every item in
//! that file/module, as rustc does), PyO3 `#[pyclass]` types (Python extension
//! objects whose debug surface is Python's `__repr__`/`__str__`, not Rust
//! `Debug`), types with
//! raw-pointer fields, and types that store a closure/function in a field
//! whose generic type parameter carries an `Fn`/`FnMut`/`FnOnce` bound (the
//! combinator pattern in poem/tower/axum — closures don't implement `Debug`,
//! so neither a derive nor a `Debug`-bounded field is viable).
//!
//! We accept manual impls because libraries with closure or PhantomData
//! fields legitimately can't derive — they hand-roll the impl. The manual
//! impl is matched on the AST trait/target of every `impl_item`, so generic
//! impls (`impl<T: Debug> Debug for Wrapper<'_, T>`) and any trait-path spelling
//! (`Debug`, `fmt::Debug`, `std::fmt::Debug`, `core::fmt::Debug`) are
//! recognized. The search spans every file of the type's crate (the crate
//! identified by the nearest `Cargo.toml`), so an impl written in a sibling
//! file (anyhow declares `pub struct Error` in `lib.rs` and `impl Debug for
//! Error` in `error.rs`) counts.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{
    has_clippy_allow, has_doc_hidden, has_outer_attribute_path, has_test_attribute,
    is_effectively_pub, is_in_test_context,
};

/// PyO3 `#[pyclass]` attribute spellings: the bare form and the fully-qualified
/// path. A type carrying it is a Python extension class whose debug surface is
/// the Python `__repr__`/`__str__` protocol, not Rust's `Debug`.
const PYCLASS_ATTRS: &[&str] = &["pyclass", "pyo3::pyclass"];

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
        // Effective visibility is the product of the item's own `pub` modifier
        // and every enclosing module's. A bare-`pub` item confined to a
        // non-public module is unreachable by external consumers, so the rule's
        // "consumers can't log it" rationale does not apply. `is_effectively_pub`
        // walks ancestor `mod_item` nodes for the inline form
        // (`mod priv { pub struct … }`) AND, via `path`, resolves the cross-file
        // form where the file is pulled in by a non-`pub` `mod foo;` in its parent.
        if !is_effectively_pub(node, source_bytes, ctx.path) {
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
        // A binary-only crate (no `[lib]` target, no `src/lib.rs`) has no external
        // consumers, so "consumers can't debug it" is structurally inapplicable.
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.is_binary_only())
        {
            return;
        }
        if has_doc_hidden(node, source_bytes) {
            return;
        }
        // A PyO3 `#[pyclass]` type is a Python extension object: its public-facing
        // debug surface is Python's `__repr__`/`__str__` (defined in
        // `#[pymethods]`), a distinct contract from Rust's `Debug`. PyO3 itself
        // does not require `Debug`, so the rule's premise — "Rust consumers can't
        // log it" — does not apply.
        if has_outer_attribute_path(node, source_bytes, PYCLASS_ATTRS) {
            return;
        }
        // The author opted out of the rustc lint this rule mirrors, at item,
        // enclosing-module, or file/module scope (`#[allow(...)]` /
        // `#![allow(...)]`).
        if has_clippy_allow(node, source_bytes, "missing_debug_implementations") {
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
        // A hand-written `Debug` impl for this type in another file of the same
        // crate (anyhow splits `pub struct Error` in lib.rs from `impl Debug for
        // Error` in error.rs).
        if ctx.project.crate_has_manual_debug_impl(ctx.path, name) {
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

/// True if a preceding `attribute_item` sibling is `#[cfg(doctest)]`.
///
/// A `#[cfg(doctest)]` item is compiled only when rustdoc collects doctests —
/// the README-doctest harness (`#[doc = include_str!("../README.md")]
/// #[cfg(doctest)] pub struct ReadmeDoctests;`) — so it never exists in normal
/// builds and is unreachable by consumers. It is the same class of build-gated,
/// non-API item as `#[cfg(test)]`.
///
/// Walks preceding siblings like `has_doc_hidden`, skipping interleaved
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
/// token. Matches on the AST path child (`cfg`) and on the `identifier` tokens
/// inside the token tree
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
        // Use an absolute path with no Cargo.toml ancestor so the binary-only and
        // proc-macro manifest guards don't accidentally pick up comply's own
        // (binary-only) Cargo.toml.
        crate::rules::test_helpers::run_rule(&Check, source, "/nonexistent_cargo_project/src/t.rs")
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

    /// Write `parent_rel` and `child_rel` into a temp crate, then run the rule on
    /// the child so `is_effectively_pub` can read the parent's `mod` declaration
    /// off disk (the split-file private-module case).
    fn run_split_module(
        parent_rel: &str,
        parent_src: &str,
        child_rel: &str,
        child_src: &str,
    ) -> Vec<Diagnostic> {
        use std::fs;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        for rel in [parent_rel, child_rel] {
            if let Some(parent) = std::path::Path::new(rel).parent() {
                fs::create_dir_all(dir.path().join(parent)).unwrap();
            }
        }
        fs::write(dir.path().join(parent_rel), parent_src).unwrap();
        let child_path = dir.path().join(child_rel);
        fs::write(&child_path, child_src).unwrap();
        crate::rules::test_helpers::run_rule(&Check, child_src, &child_path)
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

    /// Closes #5289 (issue repro): eyre/src/kind.rs opens with a file-level inner
    /// attribute `#![allow(missing_debug_implementations)]` that silences the
    /// rustc lint for *every* item in the file. The inner attribute is a child of
    /// `source_file`, not a preceding sibling of the struct, so the file-scope
    /// scan must catch it.
    #[test]
    fn suppresses_file_level_inner_allow_missing_debug() {
        let source = "#![allow(missing_debug_implementations, missing_docs)]\n\
                      pub struct Adhoc;\n\
                      pub struct Trait;\n\
                      pub struct Boxed;";
        assert!(
            run_on(source).is_empty(),
            "a file-level #![allow(missing_debug_implementations)] must suppress every item"
        );
    }

    /// A module-level inner attribute `#![allow(missing_debug_implementations)]`
    /// inside a `mod` block suppresses every item in that module.
    #[test]
    fn suppresses_module_level_inner_allow_missing_debug() {
        let source = "pub mod kind {\n\
                      \x20   #![allow(missing_debug_implementations)]\n\
                      \x20   pub struct Adhoc;\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "a module-level inner #![allow(...)] must suppress every item in that module"
        );
    }

    /// An outer `#[allow(missing_debug_implementations)]` on an enclosing `mod`
    /// block suppresses every item inside it (as rustc does).
    #[test]
    fn suppresses_outer_allow_on_enclosing_mod() {
        let source = "#[allow(missing_debug_implementations)]\n\
                      pub mod kind {\n\
                      \x20   pub struct Adhoc;\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "an outer #[allow(...)] on an enclosing mod must suppress every item inside"
        );
    }

    /// Item-level outer `#[allow(missing_debug_implementations)]` (the #3980
    /// case) still suppresses the directly-annotated item.
    #[test]
    fn suppresses_item_level_outer_allow_missing_debug() {
        assert!(
            run_on("#[allow(missing_debug_implementations)]\npub struct Adhoc;").is_empty(),
            "an item-level #[allow(...)] must suppress that item"
        );
    }

    /// Load-bearing negative: in a file WITHOUT the allow, a public type with no
    /// Debug is still flagged — the file/module-scope scan must not over-suppress.
    #[test]
    fn still_flags_public_type_in_file_without_allow() {
        assert_eq!(
            run_on("#![allow(missing_docs)]\npub struct Adhoc { name: String }").len(),
            1,
            "a file-level allow of an unrelated lint must not suppress missing-debug"
        );
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

    const BINARY_ONLY_CARGO_TOML: &str = r#"
[package]
name = "c"
version = "0.1.0"
edition = "2021"
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

    /// Closes #4375: a `pub` type in a binary-only crate (no `[lib]` target, no
    /// `src/lib.rs`, as in ducaale/xh's `Session`) is unreachable by any external
    /// consumer, so it must not be flagged.
    #[test]
    fn suppresses_pub_type_in_binary_only_crate() {
        assert!(
            run_on_with_cargo(BINARY_ONLY_CARGO_TOML, "pub struct Session { url: u32 }")
                .is_empty(),
            "must not flag pub types in a binary-only crate"
        );
        assert!(
            run_on_with_cargo(BINARY_ONLY_CARGO_TOML, "pub enum Buffer { Stdout, Stderr }")
                .is_empty(),
            "must not flag pub enums in a binary-only crate"
        );
    }

    /// Closes #5784 (issue repro): a PyO3 `#[pyclass]` enum is a Python
    /// extension object — Python uses `__repr__`/`__str__`, not Rust `Debug` —
    /// so it must not be flagged for a missing `Debug` impl.
    #[test]
    fn suppresses_pyclass_enum() {
        let source = "#[pyclass]\npub enum PySign { Positive, Negative }";
        assert!(
            run_on(source).is_empty(),
            "a #[pyclass] enum must not be flagged for missing Debug"
        );
    }

    /// Closes #5784: the parametrized form `#[pyclass(name = "UBig")]` (dashu's
    /// `UPy`) must also be exempted — the guard keys on the attribute path, not
    /// its arguments.
    #[test]
    fn suppresses_parametrized_pyclass_struct() {
        let source = "#[derive(Clone)]\n#[pyclass(name = \"UBig\")]\npub struct UPy(pub u32);";
        assert!(
            run_on(source).is_empty(),
            "a #[pyclass(name = ...)] struct must not be flagged for missing Debug"
        );
    }

    /// The fully-qualified `#[pyo3::pyclass]` path form is the same Python
    /// extension class and must also be exempted.
    #[test]
    fn suppresses_qualified_pyo3_pyclass_struct() {
        let source = "#[pyo3::pyclass]\npub struct Wrapper(pub u32);";
        assert!(
            run_on(source).is_empty(),
            "a #[pyo3::pyclass] struct must not be flagged for missing Debug"
        );
    }

    /// Load-bearing negative: an ordinary public type with no `#[pyclass]` is a
    /// Rust-facing type and must still be flagged — the exemption is
    /// `#[pyclass]`-specific, not a blanket carve-out.
    #[test]
    fn still_flags_plain_pub_struct_without_pyclass() {
        assert_eq!(
            run_on("pub struct Api { name: String }").len(),
            1,
            "a public type without #[pyclass] must still flag"
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

    /// Build a Rust crate on disk from `(relative_path, source)` files, index it
    /// via `ProjectCtx::for_test_with_files`, and run the rule on `entry` (a path
    /// relative to the crate root) with that cross-file project. Lets a manual
    /// `Debug` impl in a sibling file be recognized.
    fn run_cross_file(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
        use crate::files::{Language, SourceFile};
        use std::fs;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let mut source_files = Vec::new();
        let mut entry_source = String::new();
        let mut entry_path = dir.path().to_path_buf();
        for (rel, source) in files {
            let path = dir.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, source).unwrap();
            if *rel == entry {
                entry_source = (*source).to_owned();
                entry_path = path.clone();
            }
            let language = if rel.ends_with(".rs") {
                Language::Rust
            } else {
                Language::Toml
            };
            source_files.push(SourceFile { path, language });
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let project = crate::project::ProjectCtx::for_test_with_files(&refs);
        crate::rules::test_helpers::run_ast_check(
            &Check,
            &entry_source,
            &entry_path,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    const SIBLING_LIB_CARGO_TOML: &str = r#"
[package]
name = "anyhow-like"
version = "0.1.0"
edition = "2021"

[lib]
name = "anyhow_like"
"#;

    /// Closes #4473: anyhow declares `pub struct Error` in `src/lib.rs` but
    /// hand-writes its `Debug` impl in `src/error.rs`. The cross-file crate scan
    /// must recognize the sibling-file impl, so `Error` is not flagged.
    #[test]
    fn allows_manual_debug_impl_in_sibling_file_of_same_crate() {
        let lib = "pub struct Error { inner: u32 }";
        let error = "use std::fmt;\n\
                     impl fmt::Debug for Error {\n\
                         fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) }\n\
                     }";
        let diags = run_cross_file(
            &[
                ("Cargo.toml", SIBLING_LIB_CARGO_TOML),
                ("src/lib.rs", lib),
                ("src/error.rs", error),
            ],
            "src/lib.rs",
        );
        assert!(
            diags.is_empty(),
            "a manual Debug impl in a sibling file of the same crate must be recognized"
        );
    }

    /// Load-bearing keying check: a `Debug` impl for `Error` in a *different*
    /// crate must not exempt the `Error` of this crate. The two structs live
    /// under separate `Cargo.toml`s, so the cross-file index keys them apart.
    #[test]
    fn still_flags_when_debug_impl_is_in_a_different_crate() {
        let a_lib = "pub struct Error { inner: u32 }";
        let b_lib = "use std::fmt;\n\
                     impl fmt::Debug for Error {\n\
                         fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) }\n\
                     }";
        let diags = run_cross_file(
            &[
                ("a/Cargo.toml", SIBLING_LIB_CARGO_TOML),
                ("a/src/lib.rs", a_lib),
                ("b/Cargo.toml", SIBLING_LIB_CARGO_TOML),
                ("b/src/lib.rs", b_lib),
            ],
            "a/src/lib.rs",
        );
        assert_eq!(
            diags.len(),
            1,
            "an impl Debug for Error in crate B must not exempt crate A's Error"
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

    /// Closes #6383: a `pub struct` inside a private (non-`pub`) inline module is
    /// unreachable by external consumers — effective visibility is the product of
    /// the item's modifier and every enclosing module's — so the missing-Debug
    /// rationale does not apply (thiserror `display.rs`
    /// `mod placeholder { pub struct Placeholder; }`).
    #[test]
    fn suppresses_pub_struct_in_private_inline_module() {
        let source = "mod placeholder {\n    pub struct Placeholder;\n}";
        assert!(
            run_on(source).is_empty(),
            "a pub struct inside a private inline module must not be flagged"
        );
    }

    /// Load-bearing negative: a `pub struct` inside a `pub mod` whose chain to the
    /// crate root is fully public IS part of the public API and must still flag —
    /// the effective-visibility gate must not gut the rule.
    #[test]
    fn still_flags_pub_struct_in_public_inline_module() {
        let source = "pub mod m {\n    pub struct S { x: u32 }\n}";
        assert_eq!(
            run_on(source).len(),
            1,
            "a pub struct inside a fully-public module chain must still flag"
        );
    }

    /// Load-bearing negative: a top-level `pub struct` with no enclosing module is
    /// public API and must still flag.
    #[test]
    fn still_flags_top_level_pub_struct() {
        assert_eq!(run_on("pub struct Var { x: u32 }").len(), 1);
    }

    /// Closes #6383: thiserror declares `mod var;` (non-`pub`) in `src/lib.rs`, so
    /// the `pub struct Var` in `src/var.rs` is unreachable by external consumers
    /// even though the file is parsed standalone. `is_effectively_pub` resolves
    /// the parent `mod` declaration from the path and must suppress it.
    #[test]
    fn suppresses_pub_struct_in_split_file_private_module() {
        let diags = run_split_module(
            "src/lib.rs",
            "mod var;\n",
            "src/var.rs",
            "pub struct Var<'a, T: ?Sized>(pub &'a T);\n",
        );
        assert!(
            diags.is_empty(),
            "a pub struct in a file included via a private `mod var;` must not be flagged: {diags:?}"
        );
    }

    /// Load-bearing negative: the identical struct in a file included via
    /// `pub mod var;` (a public chain) IS public API and must still flag — the
    /// suppression triggers only on a proven non-public parent declaration.
    #[test]
    fn still_flags_pub_struct_in_split_file_public_module() {
        let diags = run_split_module(
            "src/lib.rs",
            "pub mod var;\n",
            "src/var.rs",
            "pub struct Var<'a, T: ?Sized>(pub &'a T);\n",
        );
        assert_eq!(
            diags.len(),
            1,
            "a pub struct in a file included via `pub mod var;` must still flag: {diags:?}"
        );
    }
}
