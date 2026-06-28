//! Shared helpers for Rust tree-sitter rules.
//!
//! Extracted because three independent rules need the same
//! "are we inside an async function" check (`thread-sleep-in-async`,
//! `block-on-in-async`, `sync-io-in-async`). Rule of three: extract.

use std::path::{Component, Path, PathBuf};

use tree_sitter::Node;

/// True if `node` is inside an `async fn`. Walks up parents looking
/// for the nearest `function_item` and inspects its `function_modifiers`
/// child for the `async` keyword. tree-sitter-rust groups `async`,
/// `const`, `unsafe`, `extern "C"` etc. under a `function_modifiers`
/// node, so a sync function never has `async` there — even one named
/// with a raw identifier (`fn r#async()`), whose `async` lives only in
/// the `name` field, not in any modifier.
///
/// Closures (`async move { … }`) are not handled here on purpose:
/// the typical footgun is calling sync APIs from `async fn` bodies,
/// not from short-lived async blocks.
pub fn is_inside_async_fn(node: Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item" {
            return fn_is_async(parent, source);
        }
        cur = parent;
    }
    false
}

/// True if a `function_item`'s `function_modifiers` child contains the
/// `async` keyword. Scans the modifiers node only, so raw identifiers
/// (`fn r#async()`), parameter types, and return types named "async"
/// can't trip the check.
pub fn fn_is_async(function_item: Node, source: &[u8]) -> bool {
    let mut cursor = function_item.walk();
    for child in function_item.children(&mut cursor) {
        if child.kind() == "function_modifiers" {
            return child
                .utf8_text(source)
                .is_ok_and(|text| text.split_whitespace().any(|word| word == "async"));
        }
    }
    false
}

/// True if `node` sits in a const-evaluated context, where `for` loops and
/// iterators are unavailable (`for` desugars to `IntoIterator::into_iter`,
/// which is not `const`). A manual `while`-index loop is then the only way to
/// express bounded iteration.
///
/// Walks up parents and exempts when the loop is either:
///
/// - inside a `const_item` / `static_item` initializer block, or
/// - inside a `const fn` (a `function_item` whose `function_modifiers` child
///   carries the `const` keyword).
///
/// The walk stops at the first enclosing `function_item` that is NOT a
/// `const fn` (a normal runtime body re-enables the lint) and at closure
/// boundaries (`closure_expression`), so a runtime loop nested in a module
/// alongside a `const` is unaffected.
pub fn is_in_const_eval_context(node: Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "const_item" | "static_item" => return true,
            "function_item" => return fn_modifiers_contain_const(parent, source),
            "closure_expression" => return false,
            _ => {}
        }
        cur = parent;
    }
    false
}

/// True if a `function_item`'s `function_modifiers` child contains the `const`
/// keyword. Scans the modifiers node only, so raw identifiers (`fn r#const()`),
/// parameter names, and types named "const" can't trip the check.
fn fn_modifiers_contain_const(function_item: Node, source: &[u8]) -> bool {
    let mut cursor = function_item.walk();
    for child in function_item.children(&mut cursor) {
        if child.kind() == "function_modifiers" {
            return child
                .utf8_text(source)
                .is_ok_and(|text| text.split_whitespace().any(|word| word == "const"));
        }
    }
    false
}

/// True when `cast` (a `type_cast_expression`) lies in a const-evaluation
/// context where trait-based conversions are unavailable, so `as` is the only
/// conversion the language offers and the `as`-cast lints have no valid
/// remediation to suggest.
///
/// In a const-evaluated position the rules' usual alternatives do not compile:
/// `From::from` is not implemented for signed↔unsigned integer pairs
/// (`u64::from(i32::MIN)` is rejected), and `TryFrom`/`TryInto` are not
/// const-stable. The cast is therefore mandatory, making the diagnostic a
/// guaranteed false positive.
///
/// Walks up parents and decides at the first enclosing const-relevant node:
///
/// - `const_item` / `static_item`: const when the subtree it ascended through
///   is the item's `value` field (the initializer after `=`), so the type
///   annotation is not exempted;
/// - `function_item`: const iff that function carries the `const` modifier (a
///   `const fn` body is fully const-evaluated; a normal runtime body is not);
/// - `array_type` / `array_expression`: const when ascended through the
///   `length` field — an array-length type (`[u8; N as usize]`) or
///   array-repeat count (`[0u8; N as usize]`);
/// - `const_block`: a `const { … }` block is const-evaluated;
/// - `type_arguments`: a const-generic argument (`Foo<{ X as usize }>`), where
///   the cast is part of a compile-time generic argument.
///
/// The walk stops at the first `closure_expression` (closures are not const)
/// and at any non-const `function_item`, so a runtime cast nested in a module
/// alongside a `const` keeps being flagged.
///
/// Shared by `rust-no-as-numeric-cast` and `rust-no-lossy-as-cast`.
pub fn cast_in_const_context(cast: Node, source: &[u8]) -> bool {
    let mut cur = cast;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "const_item" | "static_item" => {
                return parent.child_by_field_name("value") == Some(cur);
            }
            "array_type" | "array_expression" => {
                if parent.child_by_field_name("length") == Some(cur) {
                    return true;
                }
            }
            "const_block" => return true,
            "type_arguments" => return true,
            "function_item" => return fn_modifiers_contain_const(parent, source),
            "closure_expression" => return false,
            _ => {}
        }
        cur = parent;
    }
    false
}

/// True if `node` is the discriminant initializer of an enum variant — the
/// expression after `=` in `Variant = <expr>` (tree-sitter-rust: the `value`
/// field of an `enum_variant`).
///
/// A discriminant must be a constant expression, where `as` is the only
/// conversion that compiles: `From`/`TryFrom` are unavailable (`i8: From<u8>`
/// is not implemented, and `TryInto`/`TryFrom` are not const-stable), so the
/// `as`-cast lints have no valid remediation to offer there.
///
/// Walks up parents and, at the first enclosing `enum_variant`, returns true
/// only when the subtree it ascended through is that variant's `value` field
/// (so `(b's' as i8) + 1` is covered too). The walk stops at a `function_item`
/// / `closure_expression` boundary, so a cast inside an `impl Enum` method —
/// which is a runtime body, not a discriminant — keeps being flagged.
pub fn is_in_enum_discriminant(node: Node) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "enum_variant" => return parent.child_by_field_name("value") == Some(cur),
            "function_item" | "closure_expression" => return false,
            _ => {}
        }
        cur = parent;
    }
    false
}

/// True if `node` is in the initializer (the `value` field) of a `const` or
/// `static` item — the expression after `=` in `const NAME: T = <expr>;`.
///
/// A const/static item initializer is const-evaluated at compile time: a
/// `None`/`Err` there is a compile-time error, not a runtime panic. None of the
/// usual fallibility remediations apply — `?` does not compile (a const item is
/// not a function body), `unwrap_or_else` closures are not const-callable, and a
/// const item cannot evaluate to a `Result`. `unwrap`/`expect` are the only
/// const-stable, safe way to extract the value, so the panic-family lints have
/// nothing valid to offer there.
///
/// Walks up parents and, at the first enclosing `const_item` / `static_item`,
/// returns true only when the subtree it ascended through is that item's `value`
/// field (so the type annotation isn't exempted). The walk stops at a
/// `function_item` / `closure_expression` boundary, so a call inside a `const fn`
/// body — which is a runtime body that can return `Result` and use `?` — keeps
/// being flagged.
pub fn is_in_const_initializer(node: Node) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "const_item" | "static_item" => {
                return parent.child_by_field_name("value") == Some(cur);
            }
            "function_item" | "closure_expression" => return false,
            _ => {}
        }
        cur = parent;
    }
    false
}

/// True if `node` is inside a closure that is passed directly as an argument
/// to a thread-spawning function (`thread::spawn`, `spawn_blocking`, etc.).
/// Those closures execute on a separate OS thread, not on the async runtime
/// worker, so blocking calls inside them are safe.
pub fn is_inside_spawned_closure(node: Node, source: &[u8]) -> bool {
    use crate::rules::call_expression::call_function_name;
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item" {
            return false;
        }
        if parent.kind() == "closure_expression" {
            if let Some(args) = parent.parent()
                && args.kind() == "arguments"
                && let Some(call) = args.parent()
                && call.kind() == "call_expression"
                && let Some(fn_text) = call_function_name(call, source)
                && is_thread_spawn_fn(fn_text)
            {
                return true;
            }
        }
        cur = parent;
    }
    false
}

fn is_thread_spawn_fn(text: &str) -> bool {
    text.ends_with("thread::spawn")
        || text.contains("thread::Builder")
        || text.ends_with("spawn_blocking")
        || text.ends_with("rayon::spawn")
}

/// If `node` is a `Result<T, E>` `generic_type`, return its second
/// positional type argument (the error type `E`). Returns `None` for
/// any other node, or for `Result<T>` aliases like `io::Result<T>`
/// where the error type isn't visible from the AST.
///
/// Both `rust-string-as-error` and `rust-unit-error-result` need this
/// "find the error type" walk — without it they reimplemented the
/// same generic-arg traversal in two places.
pub fn result_error_type<'a>(node: Node<'a>, source: &[u8]) -> Option<Node<'a>> {
    if node.kind() != "generic_type" {
        return None;
    }
    let type_node = node.child_by_field_name("type")?;
    let type_text = type_node.utf8_text(source).ok()?;
    if type_text != "Result" && !type_text.ends_with("::Result") {
        return None;
    }
    let args = node.child_by_field_name("type_arguments")?;
    let mut cursor = args.walk();
    let positional: Vec<_> = args
        .named_children(&mut cursor)
        .filter(|c| c.kind() != "type_binding")
        .collect();
    if positional.len() < 2 {
        return None;
    }
    Some(positional[1])
}

/// If `node` is a `Result<T, E>` `generic_type`, return its first
/// positional type argument (the ok type `T`). Returns `None` for
/// any other node, or for `Result<T>` aliases like `io::Result<T>`
/// where the error type isn't visible from the AST.
///
/// Mirrors [`result_error_type`] but yields the ok type instead of the
/// error type. `rust-unit-error-result` needs both arms to tell a pure
/// binary `Result<(), ()>` from a `Result<Value, ()>` that discards a
/// real error while still returning data.
pub fn result_ok_type<'a>(node: Node<'a>, source: &[u8]) -> Option<Node<'a>> {
    if node.kind() != "generic_type" {
        return None;
    }
    let type_node = node.child_by_field_name("type")?;
    let type_text = type_node.utf8_text(source).ok()?;
    if type_text != "Result" && !type_text.ends_with("::Result") {
        return None;
    }
    let args = node.child_by_field_name("type_arguments")?;
    let mut cursor = args.walk();
    let positional: Vec<_> = args
        .named_children(&mut cursor)
        .filter(|c| c.kind() != "type_binding")
        .collect();
    if positional.len() < 2 {
        return None;
    }
    Some(positional[0])
}

/// True when the file containing `node` declares a local `type Result<…> = …`
/// alias that shadows `std::result::Result`.
///
/// A local `Result` alias can reorder the standard `Result<T, E>` type
/// parameters — e.g. `type Result<'a, T> = core::result::Result<T, Box<Error>>`
/// puts the success type first and pins the error type. Under such an alias a
/// `Result<_, ()>` usage has the `()` in the *success* position, not the error
/// position, so the positional "second arg is the error" assumption that
/// `result_error_type` encodes no longer holds. Rules keyed on the error
/// position must not fire on `Result<…>` usages in a file that shadows `Result`.
///
/// Detection is structural: any `type_item` whose `name` is `Result` reachable
/// from the file root, including inside nested modules.
pub fn file_has_local_result_alias(node: Node, source: &[u8]) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    subtree_declares_result_alias(root, source)
}

/// Recursively true when `node` or any descendant is a `type_item` named
/// `Result`. Used by [`file_has_local_result_alias`] to spot a `Result` alias
/// anywhere in the file, including nested `mod` blocks.
fn subtree_declares_result_alias(node: Node, source: &[u8]) -> bool {
    if node.kind() == "type_item"
        && node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            == Some("Result")
    {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| subtree_declares_result_alias(child, source))
}

/// True if `node` is inside any form of Rust test context:
///
/// - inside a `#[test]` function
/// - inside a function, module, or impl block whose `cfg` predicate activates
///   `test` — `#[cfg(test)]`, `#[cfg(all(test, …))]`, `#[cfg(any(test, …))]`,
///   `#[cfg_attr(test, …)]`, and nested combinations
/// - inside a file marked with `#![cfg(test)]`
///
/// A negated predicate such as `#[cfg(not(test))]` is production-only and does
/// not count as a test context.
///
/// Rules that want to relax their discipline for test code (allow
/// `unwrap`, `panic!`, `let _ = fallible()`, etc.) call this helper
/// to decide whether a candidate should be skipped.
pub fn is_in_test_context(node: Node, source: &[u8]) -> bool {
    // File-level inner attribute: `#![cfg(test)]` on the crate root.
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() != "inner_attribute_item" {
            continue;
        }
        if let Ok(text) = child.utf8_text(source)
            && cfg_predicate_activates_test(text)
        {
            return true;
        }
    }

    // Outer `#[test]` / `#[cfg(test)]` on an enclosing function, module, or
    // impl block (a cfg-gated `impl Trait for T` is a common test-only shape).
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if matches!(parent.kind(), "function_item" | "mod_item" | "impl_item")
            && has_test_attribute(parent, source)
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// True if `node` is inside a [Kani](https://model-checking.github.io/kani/)
/// formal-verification harness — an enclosing function carrying a `kani`
/// proof attribute:
///
/// - `#[kani::proof]`
/// - `#[kani::proof_for_contract(...)]`
///
/// A Kani harness symbolically executes a function over unconstrained
/// `kani::any()` inputs to verify the absence of panics, undefined behavior,
/// and contract violations. The harness lives in the production `src/` tree
/// (not under `tests/`) yet is verification code, not shipping logic: a
/// `let _ = f(..)` there exercises the call for its safety property and
/// intentionally discards the return value.
///
/// Rules that relax their discipline for verification harnesses (allow
/// `unwrap`, `let _ = fallible()`, etc.) call this helper — the verification
/// analog of [`is_in_test_context`] — to decide whether a candidate should be
/// skipped.
pub fn is_in_kani_proof(node: Node, source: &[u8]) -> bool {
    const KANI_PROOF_ATTRS: &[&str] = &["kani::proof", "kani::proof_for_contract"];
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item"
            && has_outer_attribute_path(parent, source, KANI_PROOF_ATTRS)
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// True if a single path SEGMENT names a test directory by a `-`/`_`-delimited
/// `test`/`tests` token.
///
/// The segment is split on `-` and `_`; it qualifies when any resulting token
/// is exactly `test` or `tests`. This recognizes both snake_case and kebab-case
/// conventions (`property_tests`, `test_helpers`, `integration-tests`,
/// `test-helpers`, `e2e-tests`, `end-to-end-tests`, `test-utils`) while
/// rejecting segments where `test` is a non-delimited substring (`latest`,
/// `greatest`, `contest`, `attestation`, `testingground`).
fn segment_is_test_token_dir(segment: &str) -> bool {
    segment
        .split(['-', '_'])
        .any(|token| token == "test" || token == "tests")
}

/// True if `path` is test infrastructure recognizable by path or file name,
/// independent of any `#[cfg(test)]` attribute.
///
/// A file qualifies when either:
///
/// - any path SEGMENT contains a `-`/`_`-delimited `test`/`tests` token
///   (`tests`, `property_tests`, `test_helpers`, `integration-tests`,
///   `test-helpers`, `e2e-tests`, …) — covers Cargo's `tests/` integration
///   directory, integration-test crates, and shared test-helper modules at any
///   nesting depth in both snake_case and kebab-case; OR
/// - any path SEGMENT is exactly `testing` or `testutil` (where `test` is a
///   prefix, not a delimited token); OR
/// - the file NAME is exactly `tests.rs`, `test.rs`, `testing.rs`,
///   `test_utils.rs`, `test_helpers.rs`, or `testutil.rs`. `tests.rs` / `test.rs`
///   is the idiomatic Rust inline-test-module convention (`mod tests;` in a
///   parent source file resolves to a sibling `tests.rs` holding `#[test]`
///   functions and their helpers).
///
/// Cross-crate test helpers cannot be `#[cfg(test)]` (that gate hides them
/// from integration tests in *other* crates), so their test-only nature is
/// conveyed by path and name instead. The `test`/`tests` token must be
/// delimited by segment boundaries or `-`/`_`, never a bare substring:
/// `latest/`, `greatest/`, `testingground/`, and `my_testing.rs` are
/// production code and do not qualify.
///
/// Shared by Rust rules that relax their discipline (allow `unwrap`,
/// `panic!`, …) for test infrastructure without relying on the tree-sitter
/// attribute walk.
pub fn is_under_tests_dir(path: &std::path::Path) -> bool {
    const TEST_SEGMENTS: &[&str] = &["testing", "testutil"];
    const TEST_FILE_NAMES: &[&str] = &[
        "tests.rs",
        "test.rs",
        "testing.rs",
        "test_utils.rs",
        "test_helpers.rs",
        "testutil.rs",
    ];

    if path.components().any(|c| {
        c.as_os_str().to_str().is_some_and(|seg| {
            segment_is_test_token_dir(seg) || TEST_SEGMENTS.contains(&seg)
        })
    }) {
        return true;
    }
    path.file_name()
        .is_some_and(|name| TEST_FILE_NAMES.iter().any(|test_name| name == *test_name))
}

/// True if `node` is inside a function literally named `main`. Walks up
/// the ancestors looking for an enclosing `function_item` whose `name`
/// field is `main` — the binary entry point, where errors are printed to
/// stderr and never cross a thread boundary.
pub fn is_in_fn_main(node: Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item"
            && let Some(name) = parent.child_by_field_name("name")
            && let Ok(t) = name.utf8_text(source)
            && t == "main"
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// True if the item has a test-marking attribute as a preceding
/// `attribute_item` sibling. In tree-sitter-rust, outer attributes on an item
/// appear as `attribute_item` nodes immediately before the item they decorate.
///
/// Recognized forms:
///
/// - `#[test]`
/// - path test macros: `#[tokio::test]`, `#[actix_rt::test(…)]`, …
/// - `cfg` / `cfg_attr` predicates where `test` is an active configuration
///   predicate: `#[cfg(test)]`, `#[cfg(all(test, …))]`, `#[cfg(any(test, …))]`,
///   `#[cfg_attr(test, …)]`, and arbitrary nesting such as
///   `#[cfg(all(feature = "std", any(test, fuzzing)))]`.
///
/// A `test` predicate negated by `not(…)` (e.g. `#[cfg(not(test))]`) is
/// production-only and is *not* treated as a test attribute.
///
/// Doc comments (`///`, `/** … */`) may interleave the attributes and the item
/// in any order; they are skipped, not treated as the end of the attribute
/// block.
pub fn has_test_attribute(item: Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                if let Ok(text) = s.utf8_text(source)
                    && attr_marks_test(text)
                {
                    return true;
                }
            }
            // Doc comments may interleave the attributes; skip them (see docblock).
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if a single attribute's source text marks test code: a `#[test]` /
/// path test macro, or a `cfg`/`cfg_attr` whose predicate activates `test`
/// (positively, outside any `not(…)`). See `has_test_attribute`.
fn attr_marks_test(text: &str) -> bool {
    text.contains("#[test]")
        || text.contains("::test]")   // #[tokio::test], #[actix_rt::test], …
        || text.contains("::test(")   // #[tokio::test(flavor = "multi_thread")], …
        || cfg_predicate_activates_test(text)
}

/// True when an enclosing scope carries `#[allow(clippy::<lint>)]` or
/// `#[expect(clippy::<lint>)]` naming one of `lints` (each given WITHOUT the
/// `clippy::` prefix, e.g. `"result_unit_err"`). Walks the same ancestry as
/// `is_in_test_context`: crate-root inner attributes (`#![allow(...)]`) plus
/// outer attributes on an enclosing function / mod / impl / struct / field, or
/// on an enclosing statement (a statement-level `#[allow(...)] { … }` block or
/// `#[allow(...)] <stmt>;`). Lets a comply rule that mirrors a clippy lint
/// honor the author's in-source suppression of it — including a struct-level or
/// field-level `#[allow]` covering a type written in that struct's field, or a
/// statement-scoped `#[allow]` covering the node it decorates.
pub fn is_suppressed_by_clippy_allow(node: Node, lints: &[&str], source: &[u8]) -> bool {
    // Crate-root inner attributes: `#![allow(clippy::X)]`.
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "inner_attribute_item"
            && let Ok(text) = child.utf8_text(source)
            && attr_allows_clippy_lint(text, lints)
        {
            return true;
        }
    }
    // Outer attributes on an enclosing function / mod / impl / struct / field,
    // and statement-level attributes on an enclosing statement. In every case
    // the `#[allow(...)]` parses as a preceding `attribute_item` sibling: of the
    // decorated item (a field's attribute is a sibling inside the
    // `field_declaration_list`, not a child), or — for a statement-scoped
    // `#[allow(...)] { … }` / `#[allow(...)] expr;` — of the statement node
    // itself. Scanning each ancestor's own preceding siblings covers both.
    let mut cur = node;
    loop {
        if item_has_clippy_allow(cur, lints, source) {
            return true;
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => return false,
        }
    }
}

/// True if any `attribute_item` immediately preceding `item` is an
/// `#[allow(clippy::X)]` / `#[expect(clippy::X)]` naming one of `lints`.
/// Mirrors `has_test_attribute`'s preceding-sibling scan.
fn item_has_clippy_allow(item: Node, lints: &[&str], source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && attr_allows_clippy_lint(text, lints)
        {
            return true;
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if `text` is an `allow`/`expect` attribute naming `clippy::<lint>` for
/// one of `lints`.
fn attr_allows_clippy_lint(text: &str, lints: &[&str]) -> bool {
    let is_allow_or_expect = text.contains("allow(")
        || text.contains("expect(")
        || text.contains("allow (")
        || text.contains("expect (");
    is_allow_or_expect && lints.iter().any(|lint| text.contains(&format!("clippy::{lint}")))
}

/// True if `text` contains a `cfg(…)` / `cfg_attr(…)` predicate in which the
/// `test` configuration option appears as a positive standalone predicate.
///
/// `test` is "positive" when it is not lexically inside a `not(…)` group, so
/// `all(test, …)` / `any(test, …)` (any depth) count, while `not(test)` and
/// `all(not(test), …)` do not.
fn cfg_predicate_activates_test(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find the start of a `cfg(` or `cfg_attr(` predicate.
        if let Some(open) = cfg_arg_open(text, &mut i) {
            if test_active_in_group(bytes, open) {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

/// If a `cfg(` / `cfg_attr(` token begins at the byte cursor `*i`, advance `*i`
/// past the keyword and opening paren and return the index of the first byte
/// inside the parentheses. Otherwise return `None` without moving `*i`.
///
/// Returns `None` at a byte that is not a UTF-8 char boundary (the interior of a
/// multi-byte character): the ASCII `cfg(` / `cfg_attr(` keywords can only begin
/// on a char boundary, so skipping such a byte loses no match, and the caller
/// advances `*i` past it.
fn cfg_arg_open(text: &str, i: &mut usize) -> Option<usize> {
    if !text.is_char_boundary(*i) {
        return None;
    }
    for keyword in ["cfg_attr(", "cfg("] {
        if text[*i..].starts_with(keyword) {
            *i += keyword.len();
            return Some(*i);
        }
    }
    None
}

/// Scan a parenthesized cfg predicate group starting at byte `start` (the first
/// byte inside the opening paren) up to its matching close paren, returning true
/// if a positive `test` identifier appears outside any `not(…)`.
fn test_active_in_group(bytes: &[u8], start: usize) -> bool {
    // One entry per currently-open paren: true if that group is a `not(…)`.
    // Pushed for the implicit `cfg(`/`cfg_attr(` paren we are already inside.
    let mut negation_stack = vec![false];
    let mut pending_not = false;
    let mut i = start;
    while i < bytes.len() && !negation_stack.is_empty() {
        let b = bytes[i];
        if is_ident_byte(b) {
            let word_start = i;
            while i < bytes.len() && is_ident_byte(bytes[i]) {
                i += 1;
            }
            let word = &bytes[word_start..i];
            if word == b"not" {
                pending_not = true;
            } else {
                if word == b"test" && !negation_stack.iter().any(|negated| *negated) {
                    return true;
                }
                pending_not = false;
            }
            continue;
        }
        match b {
            b'(' => {
                negation_stack.push(pending_not);
                pending_not = false;
            }
            b')' => {
                negation_stack.pop();
            }
            b if b.is_ascii_whitespace() => {}
            _ => {
                pending_not = false;
            }
        }
        i += 1;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The nearest enclosing `function_item` ancestor of `node`, or `None` when
/// `node` is not inside any function body (e.g. a free `const`/`static`
/// initializer at module scope).
///
/// Walks up via `node.parent()` and returns the first `function_item` found.
/// Rules that need to inspect the surrounding function as a whole — its name,
/// body, or the literals it contains — use this instead of re-implementing the
/// walk.
pub fn enclosing_fn(node: Node) -> Option<Node> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item" {
            return Some(parent);
        }
        cur = parent;
    }
    None
}

/// True if `node` sits inside the body of an enclosing loop — a
/// `for_expression`, `while_expression`, or `loop_expression` — within the
/// current function or closure scope.
///
/// The walk goes up via `node.parent()` and returns `true` on the first loop
/// node encountered. It stops (returning `false`) at the first
/// `function_item`, `closure_expression`, or `async_block` boundary, so a loop
/// that lives *outside* an intervening closure / spawned future does not count:
/// only a loop in the same lexical scope as `node` qualifies. A loop nested
/// *below* `node` is never seen, since the walk only moves upward.
///
/// Rules use this to recognize work that repeats per iteration — where a value
/// (a `JoinHandle`, a lock guard, an allocation) is intentionally created and
/// discarded each pass rather than retained.
pub fn is_in_loop_body(node: Node) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "for_expression" | "while_expression" | "loop_expression" => return true,
            "function_item" | "closure_expression" | "async_block" => return false,
            _ => {}
        }
        cur = parent;
    }
    false
}

/// True if `item` carries the outer attribute named `attr_path` (e.g.
/// `"track_caller"`) as a preceding `attribute_item` sibling.
///
/// In tree-sitter-rust, outer attributes on an item appear as `attribute_item`
/// nodes immediately before the item they decorate, optionally separated by
/// `line_comment`/`block_comment` siblings; those comment siblings are skipped
/// so a comment between the attribute and the item does not defeat the match.
/// The match keys on the attribute's last path segment bounded by `[`/`::` on
/// the left (`#[track_caller]`, `#[core::track_caller]`), so an unrelated
/// attribute whose name merely ends in the segment (`#[my_track_caller]`) does
/// not match.
pub fn has_outer_attribute(item: Node, source: &[u8], attr_path: &str) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => {
                if let Ok(text) = s.utf8_text(source)
                    && attr_names_path(text, attr_path)
                {
                    return true;
                }
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if an `attribute_item`'s source text names `attr_path` as its last path
/// segment, matched on the bracketed token so `#[track_caller]` and
/// `#[core::track_caller]` both count. The segment is bounded by `[`/`::` on the
/// left so `#[my_track_caller]` does not match.
fn attr_names_path(attr_text: &str, attr_path: &str) -> bool {
    attr_text.contains(&format!("[{attr_path}]"))
        || attr_text.contains(&format!("::{attr_path}]"))
}

/// True if `item` carries a rustdoc `# Panics` section as a preceding doc-comment
/// sibling — the canonical Rust API convention for documenting that a function
/// may panic (per the std library API guidelines).
///
/// In tree-sitter-rust, doc comments (`///`, `/** */`) on an item appear as
/// `line_comment`/`block_comment` siblings immediately before it, possibly
/// interleaved with `attribute_item`s (e.g. `#[track_caller]`); those attribute
/// siblings are skipped so an attribute between the doc and the item does not
/// defeat the match. Each comment line is stripped of its `///`/`//!`/`/**`/`*`
/// markers and matched against a `# Panics` markdown heading (one or more `#`
/// then `Panics`), so a non-doc `// panics` comment or prose merely mentioning
/// "panics" does not match.
pub fn has_panics_doc_section(item: Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {}
            "line_comment" | "block_comment" => {
                if let Ok(text) = s.utf8_text(source)
                    && comment_has_panics_heading(text)
                {
                    return true;
                }
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if a doc comment's text contains a `# Panics` markdown heading. Handles
/// both single-line `///` comments (one line) and multi-line `/** */` blocks (a
/// `# Panics` line among many). Only outer/inner doc comments count: a plain
/// `//`/`/* */` comment is not rustdoc, so its text is ignored.
fn comment_has_panics_heading(text: &str) -> bool {
    let is_doc = text.starts_with("///")
        || text.starts_with("//!")
        || text.starts_with("/**")
        || text.starts_with("/*!");
    if !is_doc {
        return false;
    }
    text.lines().any(|line| {
        let stripped = line.trim().trim_start_matches(['/', '*', '!']).trim();
        let Some(after_hashes) = stripped.strip_prefix('#') else {
            return false;
        };
        after_hashes.trim_start_matches('#').trim() == "Panics"
    })
}

/// True if `node` is preceded by a `// SAFETY:` / `// Safety:` comment on the
/// lines directly above it. Scans upward from the node's start row, skipping
/// blank lines, other comment lines, and outer/inner attributes (`#[cfg(...)]`,
/// `#[allow(...)]`, …), and stops at the first line of real code. tree-sitter
/// doesn't attach comments to the items they document, so the scan is by source
/// text rather than by AST sibling.
///
/// Skipping attribute lines matters for platform-conditional impls — a
/// `// SAFETY:` comment routinely sits above one or more `#[cfg(...)]`
/// attributes that gate the `unsafe impl`, and the comment still documents it.
///
/// A documented `unsafe` assertion is the convention `rust-undocumented-unsafe`
/// and `rust-unsafe-impl-without-comment` enforce; rules that flag a *kind* of
/// `unsafe` impl call this to defer to an author who has already spelled out the
/// upheld invariant.
pub fn has_adjacent_safety_comment(node: Node, source: &str) -> bool {
    let start_row = node.start_position().row;
    if start_row == 0 {
        return false;
    }
    let lines: Vec<&str> = source.lines().collect();
    let mut row = start_row;
    while row > 0 {
        row -= 1;
        let Some(line) = lines.get(row) else { break };
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("//") || trimmed.starts_with("/*") {
            if trimmed.contains("SAFETY:") || trimmed.contains("Safety:") {
                return true;
            }
            continue;
        }
        if trimmed.starts_with("#[") || trimmed.starts_with("#![") {
            continue;
        }
        break;
    }
    false
}

/// True if a comment line carries a safety marker. Two forms are
/// accepted:
///
///  - a `safety` keyword (any case) immediately followed by `:` or
///    `.` — covers `// SAFETY:`, `// Safety:`, `// safety:`,
///    `// safety.`, whose casing and terminator vary across projects;
///  - the rustdoc `# Safety` heading (`/// # Safety`), where the
///    keyword stands alone after a `#` with no terminator.
///
/// The `safety` keyword itself is always required, so an arbitrary
/// comment that merely contains the word in prose
/// (`// the safety of this depends on X`) does not count.
pub fn is_safety_marker(trimmed: &str) -> bool {
    let lower = trimmed.to_ascii_lowercase();
    // `# Safety` rustdoc heading: a `#` heading whose sole word is `safety`.
    if lower.split_once('#').is_some_and(|(_, after)| after.trim() == "safety") {
        return true;
    }
    // `safety` immediately followed by a `:` or `.` terminator.
    let mut from = 0;
    while let Some(rel) = lower[from..].find("safety") {
        let after = from + rel + "safety".len();
        if matches!(lower[after..].chars().next(), Some(':' | '.')) {
            return true;
        }
        from = after;
    }
    false
}

/// True if `item` carries a `#[doc(hidden)]` outer attribute as a preceding
/// `attribute_item` sibling. `#[doc(hidden)]` is the universal author signal
/// that an item is excluded from the documented public API.
///
/// Walks preceding `attribute_item` siblings (skipping interleaved
/// `line_comment`/`block_comment` siblings, and traversing past unrelated
/// attributes such as `#[cfg(...)]`) and matches on the AST: the `attribute`'s
/// path child must be `doc` and its `token_tree` arguments must contain a
/// `hidden` identifier token. Keying on the path child and the argument token —
/// rather than scanning raw text — means `#[doc = "hidden"]` (a doc string
/// reading "hidden") and a comment mentioning `doc(hidden)` do not match.
pub fn has_doc_hidden(item: Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => {
                if attribute_is_doc_hidden(s, source) {
                    return true;
                }
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if `attribute_item` is `#[doc(hidden)]`: its `attribute` child has path
/// `doc` and a `token_tree` argument list containing a `hidden` identifier.
///
/// `attribute_item > attribute` parses as `seq($._path, optional(arguments:
/// token_tree))`. We read the path from the attribute's first named child and
/// scan the `token_tree` for an `identifier` token equal to `hidden`, so
/// `#[doc(inline)]`, `#[doc = "…"]`, and unrelated attributes do not match.
fn attribute_is_doc_hidden(attribute_item: Node, source: &[u8]) -> bool {
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
    if path.utf8_text(source) != Ok("doc") {
        return false;
    }

    let Some(token_tree) = attribute.child_by_field_name("arguments") else {
        return false;
    };

    let mut tree_cursor = token_tree.walk();
    token_tree
        .children(&mut tree_cursor)
        .any(|tok| tok.kind() == "identifier" && tok.utf8_text(source) == Ok("hidden"))
}

/// True if `node` lives inside a `doc` attribute — `#[doc(...)]`, the crate-root
/// inner form `#![doc(...)]`, or `#[doc = "..."]`.
///
/// Walks up from `node` via `parent()`; at each ancestor that is an
/// `attribute_item` or `inner_attribute_item` it checks the `attribute` child's
/// path is `doc`. Rustdoc metadata such as `#![doc(html_logo_url = "...")]` is
/// generated-documentation configuration, not runtime code, so a string literal
/// nested in its argument list is documentation text rather than a value the
/// program acts on.
///
/// Matching on the AST path child — not raw text — means a `doc` identifier
/// appearing elsewhere (a variable named `doc`, a comment) does not match.
pub fn is_in_doc_attribute(node: Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if matches!(parent.kind(), "attribute_item" | "inner_attribute_item")
            && attribute_path_is(parent, source, "doc")
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// True if the `(inner_)attribute_item`'s `attribute` child names `attr_path` as
/// its path (the identifier before any `(...)` arguments or `= value`).
fn attribute_path_is(attribute_item: Node, source: &[u8], attr_path: &str) -> bool {
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
    path.utf8_text(source) == Ok(attr_path)
}

/// True if any preceding `attribute_item` sibling of `item` names one of
/// `attr_paths` as its attribute path (the identifier before any `(...)`
/// arguments). Unlike [`has_outer_attribute`], which matches the bracketed
/// token text and so only recognizes argument-less attributes, this keys on the
/// AST path child via [`attribute_path_is`], so argument-bearing attributes such
/// as `#[deprecated(since = "…")]` and `#[proc_macro_derive(Name, …)]` match.
///
/// Interleaved `line_comment`/`block_comment` siblings are skipped and unrelated
/// attributes are traversed, so the marker need not be the attribute nearest the
/// item.
pub fn has_outer_attribute_path(item: Node, source: &[u8], attr_paths: &[&str]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => {
                if attr_paths.iter().any(|p| attribute_path_is(s, source, p)) {
                    return true;
                }
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if `node` is covered by an `#[allow(<scope>::<lint>)]` or
/// `#[expect(<scope>::<lint>)]` attribute naming `lint`, applied to an enclosing
/// statement, expression, or item.
///
/// Walks up from `node` via `parent()`; at each ancestor it scans the preceding
/// `attribute_item` siblings (skipping interleaved comment siblings, traversing
/// past unrelated attributes such as `#[cfg(...)]`) for an `allow`/`expect`
/// attribute whose argument `token_tree` contains an `identifier` token equal to
/// `lint`. At a `mod` block or the crate root it also reads the *inner*
/// attributes (`#![allow(...)]`) that scope the whole module/file, the way the
/// rustc/clippy lint itself resolves a file- or module-level allow. The walk
/// stops at the enclosing `function_item` / `closure_expression` / `source_file`
/// boundary so an `#[allow]` on a *sibling* item far above does not leak in.
///
/// Matching on the AST path child (`allow`/`expect`) and the token-tree
/// `identifier` — not raw text — means a scope prefix like `clippy::` (which
/// tokenizes as its own `identifier`) is handled, while a lint merely ending in
/// `lint` or the name appearing inside a comment does not match.
///
/// Used by rules that overlap a clippy/rustc lint to defer to an author's
/// explicit `#[allow]`/`#[expect]` of that exact lint.
pub fn has_clippy_allow(node: Node, source: &[u8], lint: &str) -> bool {
    let mut cur = node;
    loop {
        if attribute_allows_lint_in_siblings(cur, source, lint) {
            return true;
        }
        // Inner `#![allow(...)]` scope a `mod` block or the whole file; the rule
        // mirrors rustc, which honors such a module/crate-level allow for every
        // item inside.
        if matches!(cur.kind(), "mod_item" | "source_file")
            && inner_attribute_allows_lint(cur, source, lint)
        {
            return true;
        }
        if matches!(
            cur.kind(),
            "function_item" | "closure_expression" | "source_file"
        ) {
            return false;
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => return false,
        }
    }
}

/// Scan the direct-child `inner_attribute_item` (`#![allow(...)]`) nodes of a
/// `source_file` root or a `mod_item` for an `allow`/`expect` attribute naming
/// `lint`. A `mod`'s inner attributes nest inside its `declaration_list` body;
/// the `source_file` root holds them directly.
fn inner_attribute_allows_lint(scope: Node, source: &[u8], lint: &str) -> bool {
    let body = if scope.kind() == "mod_item" {
        let mut scope_cursor = scope.walk();
        match scope
            .children(&mut scope_cursor)
            .find(|c| c.kind() == "declaration_list")
        {
            Some(list) => list,
            None => return false,
        }
    } else {
        scope
    };
    let mut cursor = body.walk();
    body.children(&mut cursor).any(|child| {
        child.kind() == "inner_attribute_item" && attribute_allows_lint(child, source, lint)
    })
}

/// Scan `node`'s preceding `attribute_item` siblings for an `allow`/`expect`
/// attribute naming `lint`, skipping interleaved comments and traversing past
/// unrelated attributes.
fn attribute_allows_lint_in_siblings(node: Node, source: &[u8], lint: &str) -> bool {
    let mut sibling = node.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => {
                if attribute_allows_lint(s, source, lint) {
                    return true;
                }
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if `attribute_item` is an `allow`/`expect` attribute whose argument list
/// names `lint`, bare or scoped (`clippy::<lint>`, `rustc::<lint>`).
///
/// `attribute_item > attribute` parses as `seq($._path, optional(arguments:
/// token_tree))`: the path is the attribute's first named child and the lint
/// names live in the `token_tree` as a flat sequence of `identifier` tokens. We
/// match on the path child being `allow`/`expect` and on an `identifier` token
/// equal to `lint`, so an unrelated `#[allow(dead_code)]` does not match and a
/// scoped `clippy::<lint>` still tokenizes its final segment as `lint`.
fn attribute_allows_lint(attribute_item: Node, source: &[u8], lint: &str) -> bool {
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

/// True if `node` sits under a statement, expression, or item gated by
/// `#[cfg(debug_assertions)]`. Such code compiles out entirely in release
/// builds, so any runtime behavior it carries (a `.unwrap()`, a panic, a
/// fallible call) has no effect on the release artifact — it is the
/// declarative equivalent of `debug_assert!`.
///
/// Walks up from `node` via `parent()`; at each ancestor it scans the preceding
/// `attribute_item` siblings (skipping interleaved comment siblings, traversing
/// past unrelated attributes) for a `#[cfg(debug_assertions)]` attribute. The
/// walk stops at the enclosing `function_item` / `closure_expression` /
/// `source_file` boundary so a `cfg` gate on a *sibling* item far above does
/// not leak in.
pub fn is_under_cfg_debug_assertions(node: Node, source: &[u8]) -> bool {
    let mut cur = node;
    loop {
        if cfg_debug_assertions_in_siblings(cur, source) {
            return true;
        }
        if matches!(
            cur.kind(),
            "function_item" | "closure_expression" | "source_file"
        ) {
            return false;
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => return false,
        }
    }
}

/// Scan `node`'s preceding `attribute_item` siblings for a
/// `#[cfg(debug_assertions)]` attribute, skipping interleaved comments and
/// traversing past unrelated attributes.
fn cfg_debug_assertions_in_siblings(node: Node, source: &[u8]) -> bool {
    let mut sibling = node.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => {
                if attribute_is_cfg_debug_assertions(s, source) {
                    return true;
                }
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if `attribute_item` is `#[cfg(debug_assertions)]`: a `cfg` attribute
/// whose `token_tree` arguments contain `debug_assertions` as a direct-child
/// `identifier` token.
///
/// `attribute_item > attribute` parses as `seq($._path, optional(arguments:
/// token_tree))`. We match on the path child being `cfg` and on a direct-child
/// `identifier` token equal to `debug_assertions`, mirroring the AST traversal
/// in `attribute_allows_lint`. Matching `debug_assertions` only as a *direct*
/// child of the `cfg` token tree excludes `#[cfg(not(debug_assertions))]`,
/// whose `debug_assertions` lives inside a nested `not(...)` token tree, and a
/// compound `#[cfg(all(debug_assertions, ...))]` (nested in `all(...)`).
fn attribute_is_cfg_debug_assertions(attribute_item: Node, source: &[u8]) -> bool {
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
        .any(|tok| tok.kind() == "identifier" && tok.utf8_text(source) == Ok("debug_assertions"))
}

/// True if `item` carries a `#[cfg(...)]` or `#[cfg_attr(...)]` conditional-
/// compilation attribute as a preceding `attribute_item` sibling. Such an item
/// is a build variant: it is only compiled under a specific feature/target/test
/// configuration. Walks preceding `attribute_item` siblings (skipping
/// interleaved `line_comment`/`block_comment` siblings and traversing past
/// unrelated attributes) and matches on the AST: the `attribute`'s path child
/// must be exactly `cfg` or `cfg_attr`. Keying on the path child — rather than
/// scanning raw text — means an attribute whose name merely ends in `cfg`, or a
/// comment mentioning `cfg`, does not match.
pub fn has_cfg_attribute(item: Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => {
                if attribute_is_cfg(s, source) {
                    return true;
                }
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// Collect the trait names from the top-level `#[derive(...)]` attributes
/// applied to `item`, an item node (`struct_item` / `enum_item`).
///
/// Walks `item`'s preceding `attribute_item` siblings and, for each whose
/// `attribute` path is exactly `derive`, extracts the comma-separated trait
/// names from its `token_tree` argument list (`Ord`, `PartialEq`, …).
///
/// Only a *top-level* `#[derive(...)]` counts — the gate is the attribute's
/// path child being `derive`. A `derive(` token nested inside another
/// attribute's arguments (`#[cfg_attr(feature = "rkyv", rkyv(derive(Ord)))]`,
/// `#[cfg_attr(test, derive(Debug))]`) is NOT collected: those generate impls
/// on a companion type or under a cfg gate, not unconditionally on `item`.
/// This avoids attributing `rkyv(derive(...))`-style nested derives to the
/// host type.
///
/// Shared by `rust-ord-partial-ord-inconsistent` and
/// `rust-hash-partial-eq-mismatch`, which compare derived against manual
/// trait impls and must not be fooled by a nested `derive(`.
pub fn collect_top_level_derives(item: Node, source: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => collect_derive_traits(s, source, &mut out),
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    out
}

/// If `attribute_item` is a top-level `#[derive(...)]` (its `attribute` path is
/// exactly `derive`), push each comma-separated trait name from its argument
/// `token_tree` into `out`. Any other attribute (`cfg_attr`, `repr`, …) is
/// ignored, so a `derive(` nested inside its arguments is never collected.
fn collect_derive_traits(attribute_item: Node, source: &[u8], out: &mut Vec<String>) {
    let mut item_cursor = attribute_item.walk();
    let Some(attribute) = attribute_item
        .children(&mut item_cursor)
        .find(|child| child.kind() == "attribute")
    else {
        return;
    };

    let Some(path) = attribute.named_child(0) else {
        return;
    };
    if path.utf8_text(source) != Ok("derive") {
        return;
    }

    let Some(token_tree) = attribute.child_by_field_name("arguments") else {
        return;
    };
    let Ok(text) = token_tree.utf8_text(source) else {
        return;
    };
    // `token_tree` text is the full `( ... )` group; strip the delimiters and
    // split the trait list on commas, mirroring how trait names are compared
    // downstream (bare names like `Ord`, `PartialEq`).
    let inner = text.trim().trim_start_matches('(').trim_end_matches(')');
    for trait_name in inner.split(',') {
        let trimmed = trait_name.trim();
        if !trimmed.is_empty() {
            out.push(trimmed.to_string());
        }
    }
}

/// True if any string, raw-string, or byte-string literal in the subtree rooted
/// at `node` contains `needle` as a substring, matched case-insensitively.
///
/// In tree-sitter-rust a byte-string literal (`b"…"`) is a `string_literal`
/// node whose `utf8_text` still includes the literal's payload, so scanning
/// `string_literal` / `raw_string_literal` node text covers byte strings too.
pub fn subtree_string_literal_contains(node: Node, source: &[u8], needle: &str) -> bool {
    let needle_lower = needle.to_ascii_lowercase();
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if matches!(current.kind(), "string_literal" | "raw_string_literal")
            && let Ok(text) = current.utf8_text(source)
            && text.to_ascii_lowercase().contains(&needle_lower)
        {
            return true;
        }
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if `node` sits inside a trait implementation (`impl Trait for Type`).
///
/// Walks up via `node.parent()` to the *nearest* enclosing `impl_item` and
/// returns whether that impl has a `trait` field. The decision is made for the
/// nearest impl only: an inherent `impl Type { … }` returns `false`, and a node
/// with no enclosing impl returns `false`. Rules use this to exempt methods
/// whose shape is forced by a trait contract (the implementor can't change it).
pub fn is_in_trait_impl(node: Node) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "impl_item" {
            return ancestor.child_by_field_name("trait").is_some();
        }
        current = ancestor.parent();
    }
    false
}

/// True if `node` sits inside a method of an `impl Index for …` or
/// `impl IndexMut for …` block.
///
/// The `Index::index` / `IndexMut::index_mut` methods return `&Self::Output` /
/// `&mut Self::Output` — a reference, never a `Result`/`Option` — so they cannot
/// propagate an error. The documented trait contract is to panic on invalid
/// access (exactly how `Vec`/`HashMap`/`BTreeMap` indexing behaves), which makes
/// `.unwrap()`/`.expect()` the idiomatic, correct implementation. Rules that
/// otherwise forbid panicking exempt these bodies.
///
/// Matches both bare `impl Index<…> for T` and path-qualified
/// `impl ops::Index<…> for T` / `impl std::ops::IndexMut<…> for T` by keying on
/// the trait name's last path segment, via the nearest enclosing `impl_item`.
pub fn is_in_index_trait_impl(node: Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "impl_item" {
            return ancestor
                .child_by_field_name("trait")
                .and_then(|t| trait_base_name(t, source))
                .is_some_and(|name| name == "Index" || name == "IndexMut");
        }
        current = ancestor.parent();
    }
    false
}

/// The trait's last path segment from an `impl_item`'s `trait` field, e.g.
/// `Index` for `Index<&str>`, `ops::Index<…>`, or `std::ops::IndexMut`.
///
/// The `trait` field is a `type_identifier` (bare `Foo`), a
/// `scoped_type_identifier` (`a::b::Foo`), or a `generic_type` wrapping either
/// (`Foo<T>`, `a::b::Foo<T>`). This unwraps the generic and resolves the final
/// segment in each case.
pub(crate) fn trait_base_name<'a>(trait_node: Node, source: &'a [u8]) -> Option<&'a str> {
    let base = if trait_node.kind() == "generic_type" {
        trait_node.child_by_field_name("type")?
    } else {
        trait_node
    };
    match base.kind() {
        "type_identifier" => base.utf8_text(source).ok(),
        "scoped_type_identifier" => base
            .utf8_text(source)
            .ok()
            .and_then(|t| t.rsplit("::").next()),
        _ => None,
    }
}

/// True if `node` sits inside a trait definition (`trait Foo { … }`).
///
/// Walks up via `node.parent()` and returns true at the first enclosing
/// `trait_item` ancestor. A trait definition fixes its method signatures as part
/// of the public API contract, so rules use this — alongside [`is_in_trait_impl`]
/// — to exempt method shapes the trait author can't change without breaking
/// callers.
pub fn is_in_trait_definition(node: Node) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "trait_item" {
            return true;
        }
        current = ancestor.parent();
    }
    false
}

/// True if `item` is publicly visible outside the crate, i.e. it carries a bare
/// `pub` visibility modifier.
///
/// Canonical semantics: ONLY bare `pub` counts as public. Restricted forms —
/// `pub(crate)`, `pub(super)`, and `pub(in path)` — are treated as NON-public,
/// because the consuming rules only care about items reachable from outside the
/// crate. The `.trim() == "pub"` comparison is whitespace-robust; the restricted
/// forms carry their `(…)` qualifier in the modifier text and never trim to
/// `"pub"`.
pub fn is_pub(item: Node, source: &[u8]) -> bool {
    let mut cursor = item.walk();
    for child in item.children(&mut cursor) {
        if child.kind() == "visibility_modifier"
            && let Ok(text) = child.utf8_text(source)
        {
            return text.trim() == "pub";
        }
    }
    false
}

/// True if `node` is nested inside a module that is not publicly visible — an
/// enclosing `mod_item` declared `pub(crate)`, `pub(super)`, `pub(in path)`, or
/// with no visibility modifier at all.
///
/// Effective visibility is the product of an item's own modifier and every
/// enclosing module's modifier: a bare-`pub` item inside a `pub(crate) mod`
/// cannot escape the crate. The walk returns true at the first ancestor
/// `mod_item` that is not bare-`pub` (reusing [`is_pub`], which treats every
/// restricted form as non-public), and false once the ancestor chain reaches
/// the file root with only bare-`pub` modules in between.
///
/// Rules whose rationale is "this reaches the crate's public API" call this to
/// skip items confined to a non-public module.
pub fn is_inside_non_public_module(node: Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "mod_item" && !is_pub(parent, source) {
            return true;
        }
        cur = parent;
    }
    false
}

/// True if `item` is effectively reachable from outside the crate: it carries a
/// bare `pub` modifier itself, no enclosing module restricts it, AND the file it
/// lives in is not itself pulled in by a non-`pub` `mod` declaration.
///
/// Effective visibility is the product of the item's own modifier and every
/// enclosing module's, so a bare-`pub` item buried in a non-public module is not
/// part of the crate's public API. That non-public module can be lexical (an
/// enclosing `mod imp { pub fn … }` in the same file — covered by
/// [`is_inside_non_public_module`]) or cross-file: the whole file is brought in
/// by a bare `mod imp;` in the parent module, so its top-level `pub` items are
/// equally unreachable. [`file_is_privately_declared_module`] resolves that
/// cross-file case from `path`.
///
/// Rules whose rationale is "this is part of the crate's public surface" gate on
/// this rather than bare [`is_pub`].
pub fn is_effectively_pub(item: Node, source: &[u8], path: &Path) -> bool {
    is_pub(item, source)
        && !is_inside_non_public_module(item, source)
        && !file_is_privately_declared_module(path)
}

/// True when the file at `path` is brought into its parent module by a
/// non-`pub` `mod` declaration — making a bare-`pub`, top-level item in it
/// unreachable from outside the crate even though it carries `pub`.
///
/// Resolution is purely structural: it locates the parent module file from the
/// path layout (`dir/foo.rs` is declared in `dir`'s module file; `dir/mod.rs`
/// is declared in the grandparent's; `lib.rs`/`main.rs` are crate roots with no
/// declarer), parses each candidate parent file with tree-sitter, and looks for
/// the external `mod` statement that pulls in THIS file — matched either by the
/// module-name stem (`mod foo;` for `foo.rs`) or by a `#[path = "…"]` /
/// `#[cfg_attr(…, path = "…")]` attribute whose resolved target is this file
/// (the platform-rename shape: `#[cfg_attr(unix, path = "unix.rs")] mod
/// platform;`).
///
/// Soundness: returns true ONLY when a declaring statement is positively
/// matched AND it is not bare-`pub`. A matched bare-`pub mod` returns false, and
/// any failure to resolve or match the parent file falls back to false — the
/// rule keeps flagging rather than suppress on a guess.
fn file_is_privately_declared_module(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };

    // The module stem to look for, and the directory whose module file carries
    // the declaring `mod` statement.
    let (decl_dir, stem): (PathBuf, String) = if file_name == "mod.rs" {
        // `dir/mod.rs` IS module `dir`; it is declared in dir's PARENT module.
        let Some(module_dir) = path.parent() else {
            return false;
        };
        let Some(stem) = module_dir.file_name().and_then(|n| n.to_str()) else {
            return false;
        };
        let Some(decl_dir) = module_dir.parent() else {
            return false;
        };
        (decl_dir.to_path_buf(), stem.to_string())
    } else if file_name == "lib.rs" || file_name == "main.rs" {
        // A crate root has no declaring `mod` statement.
        return false;
    } else {
        // `dir/foo.rs` IS module `foo`; it is declared in dir's module file.
        let Some(decl_dir) = path.parent() else {
            return false;
        };
        let Some(stem) = path.file_stem().and_then(|n| n.to_str()) else {
            return false;
        };
        (decl_dir.to_path_buf(), stem.to_string())
    };

    // Candidate files that may carry the declaration: the directory's
    // `mod.rs`/`lib.rs`/`main.rs`, plus the Rust-2018 sibling `<decl_dir>.rs`.
    let mut candidates = vec![
        decl_dir.join("mod.rs"),
        decl_dir.join("lib.rs"),
        decl_dir.join("main.rs"),
    ];
    if let (Some(parent), Some(name)) =
        (decl_dir.parent(), decl_dir.file_name().and_then(|n| n.to_str()))
    {
        candidates.push(parent.join(format!("{name}.rs")));
    }

    for candidate in candidates {
        if candidate == path {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&candidate) else {
            continue;
        };
        if let Some(declaration_is_pub) = declaring_mod_is_pub(&content, &candidate, &stem, path) {
            return !declaration_is_pub;
        }
    }
    false
}

/// Parse `content` (the parent module file at `candidate`) and return the
/// visibility of the external `mod` statement that declares `target`, or `None`
/// if no such statement is found.
///
/// `Some(true)` => declared by a bare-`pub mod`; `Some(false)` => declared by a
/// non-`pub` `mod`. Only top-level external declarations (`mod name;`, no body)
/// are considered. A `#[path]`/`#[cfg_attr(…, path)]` attribute overrides the
/// module-name stem, so a path-attributed `mod` matches only when one of its
/// resolved targets is `target`.
fn declaring_mod_is_pub(content: &str, candidate: &Path, stem: &str, target: &Path) -> Option<bool> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into()).ok()?;
    let tree = parser.parse(content, None)?;
    let decl_dir = candidate.parent()?;
    let source = content.as_bytes();

    let root = tree.root_node();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() != "mod_item" {
            continue;
        }
        // Only external declarations (`mod name;`) pull in a separate file.
        if child.child_by_field_name("body").is_some() {
            continue;
        }

        let path_targets = mod_path_attr_targets(child, source, decl_dir);
        if !path_targets.is_empty() {
            // A `#[path]` rename overrides the stem: this `mod` declares the
            // attribute's target(s), not a `<stem>.rs` sibling.
            if path_targets.iter().any(|t| paths_lexically_equal(t, target)) {
                return Some(is_pub(child, source));
            }
            continue;
        }

        if child
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            == Some(stem)
        {
            return Some(is_pub(child, source));
        }
    }
    None
}

/// Resolve every `#[path = "…"]` / `#[cfg_attr(…, path = "…")]` target carried
/// by `mod_item`'s preceding attribute siblings, relative to `decl_dir` (the
/// directory of the file declaring the module). Returns an empty vec when the
/// module carries no path-bearing attribute.
fn mod_path_attr_targets(mod_item: Node, source: &[u8], decl_dir: &Path) -> Vec<PathBuf> {
    let mut targets = Vec::new();
    let mut sibling = mod_item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                // A path attribute carries the `path` identifier and a string
                // literal naming the file; the resolved-path equality check the
                // caller performs is what actually decides the match.
                if s.utf8_text(source).is_ok_and(|text| text.contains("path")) {
                    let mut values = Vec::new();
                    collect_string_literals(s, source, &mut values);
                    for value in values {
                        // A module `#[path]` target always names a `.rs` file, so
                        // ignore the other string literals a `cfg_attr` carries
                        // (e.g. `"wasi"` from a `target_os = "wasi"` predicate).
                        if value.ends_with(".rs") {
                            targets.push(decl_dir.join(value));
                        }
                    }
                }
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    targets
}

/// Collect the unquoted value of every `string_literal` descendant of `node`
/// into `out`. Recurses through token trees so literals inside a
/// `#[cfg_attr(…, path = "…")]` argument list are reached.
fn collect_string_literals(node: Node, source: &[u8], out: &mut Vec<String>) {
    if node.kind() == "string_literal" {
        if let Some(value) = string_literal_value(node, source) {
            out.push(value.to_string());
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_string_literals(child, source, out);
    }
}

/// The unquoted text of a `string_literal` node — its `string_content` child,
/// or the literal with its surrounding quotes trimmed when empty.
fn string_literal_value<'a>(node: Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "string_content" {
            return child.utf8_text(source).ok();
        }
    }
    node.utf8_text(source).ok().map(|t| t.trim_matches('"'))
}

/// Lexical path equality that folds away `.`/`..` components, so
/// `dir/./unix.rs` and `dir/unix.rs` compare equal without touching the disk.
fn paths_lexically_equal(a: &Path, b: &Path) -> bool {
    normalize_lexically(a) == normalize_lexically(b)
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// True if the `match_arm`'s body is a single diverging or early-exit
/// expression — a `unreachable!`/`panic!`/`unimplemented!`/`todo!`/`bail!`
/// macro invocation, or a `return Err(...)` / `return None`. Such an arm is
/// an explicit guard for the impossible/error/absent case.
///
/// Two rules need this: `rust-explicit-enum-match-arms` exempts a
/// wildcard arm that only diverges, and `no-empty-catch` treats an empty
/// `Err(_) => {}` arm as a controlled assertion (not error-swallowing)
/// when a sibling arm diverges.
pub fn arm_body_is_diverging(arm: Node, source: &[u8]) -> bool {
    let Some(value) = arm.child_by_field_name("value") else {
        return false;
    };
    expr_is_diverging(value, source)
}

/// Classify a match-arm body expression as diverging/error. A `block`
/// body with a single statement is unwrapped to its inner expression so
/// `{ bail!("…"); }` is treated like `bail!("…")`.
fn expr_is_diverging(expr: Node, source: &[u8]) -> bool {
    match expr.kind() {
        "block" => {
            // Only an unconditional single-statement body is a guard:
            // `{ bail!("…"); }` or `{ return Err(e); }`. A block doing
            // other work before diverging is a real catch-all.
            let mut cursor = expr.walk();
            let mut children = expr.named_children(&mut cursor);
            let (Some(only), None) = (children.next(), children.next()) else {
                return false;
            };
            let inner = if only.kind() == "expression_statement" {
                match only.named_child(0) {
                    Some(node) => node,
                    None => return false,
                }
            } else {
                only
            };
            expr_is_diverging(inner, source)
        }
        "macro_invocation" => {
            let Some(name_node) = expr.child_by_field_name("macro") else {
                return false;
            };
            matches!(
                name_node.utf8_text(source),
                Ok("unreachable" | "panic" | "unimplemented" | "todo" | "bail")
            )
        }
        "return_expression" => return_yields_none_or_err(expr, source),
        _ => false,
    }
}

/// True if a `return_expression` returns a failure/absence value — either an
/// `Err(...)` constructor call or the `None` variant (bare `None`, or a scoped
/// `Option::None`). Both are early-exit guards for a `Result`/`Option`-returning
/// function ("this case is not handled here, propagate failure"), the same
/// structural shape, so they get the same treatment.
fn return_yields_none_or_err(ret: Node, source: &[u8]) -> bool {
    let Some(returned) = ret.named_child(0) else {
        return false;
    };
    match returned.kind() {
        // `return Err(...)`: the head of the call is the `Err` constructor.
        "call_expression" => returned
            .child_by_field_name("function")
            .and_then(|callee| callee.utf8_text(source).ok())
            .is_some_and(|text| text.rsplit("::").next().unwrap_or(text).trim() == "Err"),
        // `return None` (`identifier`) or `return Option::None` (`scoped_identifier`).
        "identifier" | "scoped_identifier" => returned
            .utf8_text(source)
            .is_ok_and(|text| text.rsplit("::").next().unwrap_or(text).trim() == "None"),
        _ => false,
    }
}

/// True if `cast` (a `type_cast_expression`) casts the result of a collection
/// size method — `<receiver>.len()`, `.count()`, or `.capacity()` — to a numeric
/// type. A Rust collection can never hold more than `isize::MAX` elements, so
/// such a value is bounded well within the range of `u32` and the other common
/// narrowing targets; forcing `try_into()` there only manufactures an
/// error path that is semantically impossible to reach.
///
/// The match is on the call shape, not on the receiver: the `function` field of
/// the cast operand must be a `field_expression` whose `field` is `len`,
/// `count`, or `capacity`, and the call must take no arguments. This rejects
/// arbitrary same-named functions taking arguments (e.g. `count(x)`) and any
/// other method-call operand, so genuinely unbounded narrowing casts stay
/// flagged.
///
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`, which both
/// otherwise flag `hunks.len() as u32` because the operand type is not resolved
/// from the AST.
pub fn cast_operand_is_collection_size(cast: Node, source: &[u8]) -> bool {
    const SIZE_METHODS: &[&str] = &["len", "count", "capacity"];

    let Some(value) = cast.child_by_field_name("value") else {
        return false;
    };
    if value.kind() != "call_expression" {
        return false;
    }
    if value
        .child_by_field_name("arguments")
        .is_some_and(|args| args.named_child_count() > 0)
    {
        return false;
    }
    let Some(function) = value.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    function
        .child_by_field_name("field")
        .and_then(|field| field.utf8_text(source).ok())
        .is_some_and(|name| SIZE_METHODS.contains(&name))
}

/// If the operand of `cast` (a `type_cast_expression`) is a bit-counting method
/// call — `<receiver>.leading_zeros()`, `.trailing_zeros()`, `.count_ones()`,
/// `.count_zeros()`, `.leading_ones()`, `.trailing_ones()` — return the maximum
/// value the call can produce (`128`).
///
/// These integer methods return a `u32` whose value is bounded by the bit-width
/// of the receiver type, at most 128 (for `u128`/`i128`). The receiver type need
/// not be resolved: 128 is the upper bound across every integer width, so it is
/// the value a caller checks against the cast target's range. Casting to any
/// integer that holds 128 (`u8`..`u128`, `i16`..`i128`) is provably lossless,
/// while a `try_into()` there only manufactures an unreachable error path.
///
/// The match is on the call shape: the `function` field of the cast operand must
/// be a `field_expression` whose `field` is one of the bit-count methods, and the
/// call must take no arguments. This rejects same-named functions taking
/// arguments and every other method-call operand, so an arbitrary unbounded
/// narrowing cast stays flagged. Arithmetic on the result
/// (`(x.leading_zeros() + offset) as u8`) is no longer a bare method-call
/// operand, so it is not matched and stays flagged.
///
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`, which both
/// otherwise flag `x.leading_zeros() as u8` because the operand's bounded range
/// is not visible from the cast in isolation.
pub fn cast_operand_bit_count_max(cast: Node, source: &[u8]) -> Option<i128> {
    const BIT_COUNT_METHODS: &[&str] = &[
        "leading_zeros",
        "trailing_zeros",
        "count_ones",
        "count_zeros",
        "leading_ones",
        "trailing_ones",
    ];

    let value = cast.child_by_field_name("value")?;
    if value.kind() != "call_expression" {
        return None;
    }
    if value
        .child_by_field_name("arguments")
        .is_some_and(|args| args.named_child_count() > 0)
    {
        return None;
    }
    let function = value.child_by_field_name("function")?;
    if function.kind() != "field_expression" {
        return None;
    }
    // The widest integer is 128 bits (`u128`/`i128`), so a bit count is ≤ 128.
    const MAX_BIT_COUNT: i128 = 128;

    let name = function.child_by_field_name("field")?.utf8_text(source).ok()?;
    BIT_COUNT_METHODS.contains(&name).then_some(MAX_BIT_COUNT)
}

/// If the operand of `cast` (a `type_cast_expression`) is a bit-reader call
/// reading a literal number of bits, return that bit count `N`.
///
/// A bit-reader `read_bits(N)` / `get_bits(N)` / `peek_bits(N)` returns a value
/// occupying at most `N` significant bits, i.e. in `0..2^N`. When `N` is a
/// literal and the cast target is wide enough to hold an `N`-bit value, the
/// narrowing cast is provably lossless — the canonical codec/bitstream parsing
/// idiom (`bs.read_bits_leq32(8)? as u8`, `r.get_bits(2)? as u8`).
///
/// The match is deliberately tight, so a genuinely unbounded cast stays flagged:
///
/// - the operand is a method call (`<receiver>.<method>(...)`) whose method name
///   *contains* `bits`, matched case-insensitively — `read_bits`, `get_bits`,
///   `peek_bits`, `read_bits_leq32`, … . Requiring `bits` in the name means the
///   single argument denotes a bit count, never a byte count (a bare `read(n)`
///   reads `n` *bytes* and is not matched);
/// - the call takes exactly one argument, a decimal `integer_literal` (`8`,
///   `16`); a non-literal count (`read_bits(n)`) yields `None` because the bound
///   is not statically known, and a non-decimal literal that `parse_int_literal`
///   cannot read yields `None`.
///
/// A `try_expression` (`...?`) and `parenthesized_expression` around the operand
/// are transparent. Returns `None` for any other operand shape.
///
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`, which both
/// otherwise flag the narrowing because the operand's bit-bounded range is not
/// visible from the cast in isolation. Callers combine it with the target width:
/// lossless iff `N <= M` for an unsigned `uM` target, `N <= M - 1` for a signed
/// `iM` target (the sign bit).
pub fn cast_operand_bit_width(cast: Node, source: &[u8]) -> Option<u16> {
    let mut value = cast.child_by_field_name("value")?;
    // `read_bits(8)?` and `(read_bits(8))` wrap the call; unwrap to the call.
    while matches!(value.kind(), "try_expression" | "parenthesized_expression") {
        value = value.named_child(0)?;
    }
    if value.kind() != "call_expression" {
        return None;
    }
    let function = value.child_by_field_name("function")?;
    if function.kind() != "field_expression" {
        return None;
    }
    let method = function
        .child_by_field_name("field")
        .and_then(|field| field.utf8_text(source).ok())?;
    if !method.to_ascii_lowercase().contains("bits") {
        return None;
    }
    let args = value.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let positional: Vec<_> = args.named_children(&mut cursor).collect();
    let [only] = positional.as_slice() else {
        return None;
    };
    if only.kind() != "integer_literal" {
        return None;
    }
    // `parse_int_literal` reads decimal digits only; a radix-prefixed literal
    // (`0xFF`, `0o17`, `0b101`) would be silently misparsed (it stops at the
    // `x`/`o`/`b`, yielding `0`). Reject those so a non-decimal count is treated
    // as not statically bounded rather than as a spurious zero-bit read.
    let text = only.utf8_text(source).ok()?;
    if matches!(text.get(..2), Some("0x" | "0o" | "0b" | "0X" | "0O" | "0B")) {
        return None;
    }
    let bits = parse_int_literal(*only, source)?;
    u16::try_from(bits).ok()
}

/// Resolve the declared type of a local binding named `name` that is visible at
/// `node`. Walks up each enclosing scope (`function_item`, `closure_expression`,
/// `block`, `source_file`) and, within it, finds the nearest binding site
/// *before* `node` that binds `name`:
///
/// - a `parameter` or `let_declaration` whose pattern binds `name` and carries an
///   explicit `type` annotation, returning that type's source text (trimmed);
/// - a `match_arm` `tuple_struct_pattern` (`Self::Left(x)`) binding `name`, whose
///   type is read from the matched enum variant's tuple field at the binding
///   position when the enum is defined in-file.
///
/// Only annotated `let`/`parameter` bindings and in-file-enum match-arm bindings
/// are resolved — an inferred `let x = ...;` or an imported-enum match arm yields
/// `None`. Shared by the numeric-cast rules, which use it to learn a cast
/// operand's source type from the AST.
pub fn find_identifier_type(node: Node, name: &str, source: &[u8]) -> Option<String> {
    let mut current = Some(node);
    while let Some(n) = current {
        if matches!(
            n.kind(),
            "function_item" | "closure_expression" | "block" | "source_file"
        ) && let Some(found) = find_binding_type_before(n, node.start_byte(), name, source)
        {
            return Some(found);
        }
        current = n.parent();
    }
    None
}

fn find_binding_type_before(node: Node, limit: usize, name: &str, source: &[u8]) -> Option<String> {
    if node.start_byte() >= limit {
        return None;
    }
    if matches!(node.kind(), "parameter" | "let_declaration")
        && let Some(pattern) = node.child_by_field_name("pattern")
        && pattern_contains_identifier(pattern, name, source)
        && let Some(type_node) = node.child_by_field_name("type")
        && let Ok(type_text) = type_node.utf8_text(source)
    {
        return Some(type_text.trim().to_string());
    }

    // A `match self { Self::Left(x) => *x as i32 }` arm binds `x` through a
    // `tuple_struct_pattern` with no type annotation; resolve its type from the
    // matched enum variant's tuple field at the binding position. Gated on the
    // arm that spans `limit` (the use site) so a same-named binding in a sibling
    // arm — each arm may rebind `x` to a different variant's field — is never the
    // one resolved here.
    if node.kind() == "tuple_struct_pattern"
        && let Some(arm) = match_arm_of_pattern(node)
        && arm.start_byte() <= limit
        && limit < arm.end_byte()
        && let Some(found) = match_arm_binding_type(node, name, source)
    {
        return Some(found);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_binding_type_before(child, limit, name, source) {
            return Some(found);
        }
    }
    None
}

fn pattern_contains_identifier(pattern: Node, name: &str, source: &[u8]) -> bool {
    if pattern.kind() == "identifier" {
        return pattern.utf8_text(source).is_ok_and(|text| text == name);
    }

    let mut cursor = pattern.walk();
    pattern
        .children(&mut cursor)
        .any(|child| pattern_contains_identifier(child, name, source))
}

/// Walk up from a `tuple_struct_pattern` through the arm's `match_pattern` wrapper
/// and any `or_pattern`, returning the enclosing `match_arm`. Returns `None` for a
/// tuple-struct pattern that is not a match-arm pattern (an `if let` / `let`
/// binding, or one wrapped in a `reference_pattern`), so resolution stays limited
/// to match arms.
fn match_arm_of_pattern(pattern: Node) -> Option<Node> {
    let mut current = pattern.parent();
    while let Some(p) = current {
        match p.kind() {
            "or_pattern" | "match_pattern" => current = p.parent(),
            "match_arm" => return Some(p),
            _ => return None,
        }
    }
    None
}

/// Resolve the type of `name` when bound by a match-arm `tuple_struct_pattern`
/// (`Self::Left(x)`), by reading the matched enum variant's tuple field at the
/// binding position. Returns the field type's source text (e.g. `"u16"`), or
/// `None` if any step can't be resolved from the current file: the variant path
/// has no resolvable enum, the enum is not defined in-file, the variant is not a
/// tuple variant, or the binding position is out of range. Never guesses.
fn match_arm_binding_type(pattern: Node, name: &str, source: &[u8]) -> Option<String> {
    let index = tuple_struct_binding_index(pattern, name, source)?;
    let path = pattern.child_by_field_name("type")?.utf8_text(source).ok()?;
    let segments: Vec<&str> = path.split("::").map(str::trim).filter(|s| !s.is_empty()).collect();
    let (variant_name, enum_segment) = match segments.as_slice() {
        [.., enum_seg, variant] => (*variant, Some(*enum_seg)),
        [variant] => (*variant, None),
        [] => return None,
    };
    let enum_name = match enum_segment {
        Some("Self") => Some(enclosing_impl_self_type(pattern, source)?),
        Some(seg) => Some(base_type_name(seg).to_string()),
        None => None,
    };
    let root = root_node(pattern);
    resolve_variant_field_type(root, enum_name.as_deref(), variant_name, index, source)
}

/// The position of `name`'s binding among a `tuple_struct_pattern`'s positional
/// sub-patterns (the `type` path is skipped). Returns `None` if a `..` rest
/// pattern precedes the binding — positional mapping would be ambiguous — or
/// `name` is not bound.
fn tuple_struct_binding_index(pattern: Node, name: &str, source: &[u8]) -> Option<usize> {
    let mut cursor = pattern.walk();
    let mut index = 0;
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            // Positional slots are the sub-patterns: the named `_pattern` children
            // (excluding the `type` path) plus the unnamed `_` wildcard, which
            // still consumes a position (`V(_, x)` binds `x` at index 1).
            // Punctuation (`(` `,` `)`) is unnamed and not `_`, so it is skipped
            // without consuming a slot.
            let is_slot = cursor.field_name() != Some("type")
                && (child.is_named() || child.kind() == "_");
            if is_slot {
                if child.kind() == "remaining_field_pattern" {
                    return None;
                }
                if pattern_contains_identifier(child, name, source) {
                    return Some(index);
                }
                index += 1;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    None
}

/// The base name of the nearest enclosing `impl_item`'s `type`, with generic
/// arguments and any path qualifier stripped (`impl path::Foo<T>` → `"Foo"`).
/// Resolves the `Self` segment of a variant path.
fn enclosing_impl_self_type(node: Node, source: &[u8]) -> Option<String> {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "impl_item" {
            let ty = n.child_by_field_name("type")?.utf8_text(source).ok()?;
            return Some(base_type_name(ty).to_string());
        }
        current = n.parent();
    }
    None
}

/// The bare type name of a possibly-qualified, possibly-generic type text: strip
/// from the first `<`, then take the final `::` segment (`a::B<C>` → `"B"`).
fn base_type_name(text: &str) -> &str {
    let head = text.split('<').next().unwrap_or(text).trim();
    head.rsplit("::").next().unwrap_or(head).trim()
}

/// The root node reached by walking up from `node` (the enclosing `source_file`).
fn root_node(node: Node) -> Node {
    let mut current = node;
    while let Some(parent) = current.parent() {
        current = parent;
    }
    current
}

/// Search the file for an `enum_item` — by name when `enum_name` is `Some`,
/// otherwise the first enum that has the variant — and return the source text of
/// its `variant_name` tuple variant's field at `index`. `None` when no such
/// in-file enum / variant / tuple-field exists.
fn resolve_variant_field_type(
    node: Node,
    enum_name: Option<&str>,
    variant_name: &str,
    index: usize,
    source: &[u8],
) -> Option<String> {
    if node.kind() == "enum_item" {
        let name_matches = match enum_name {
            Some(want) => {
                node.child_by_field_name("name").and_then(|n| n.utf8_text(source).ok()) == Some(want)
            }
            None => true,
        };
        if name_matches
            && let Some(field_type) = variant_tuple_field_type(node, variant_name, index, source)
        {
            return Some(field_type);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) =
            resolve_variant_field_type(child, enum_name, variant_name, index, source)
        {
            return Some(found);
        }
    }
    None
}

/// The source text of `enum_node`'s `variant_name` variant's tuple field at
/// `index`, or `None` when that variant is absent, is not a tuple variant
/// (fieldless or struct-style), or has no field at `index`.
fn variant_tuple_field_type(
    enum_node: Node,
    variant_name: &str,
    index: usize,
    source: &[u8],
) -> Option<String> {
    let body = enum_node.child_by_field_name("body")?;
    let mut cursor = body.walk();
    for variant in body.children(&mut cursor) {
        if variant.kind() != "enum_variant"
            || variant.child_by_field_name("name").and_then(|n| n.utf8_text(source).ok())
                != Some(variant_name)
        {
            continue;
        }
        let fields = variant.child_by_field_name("body")?;
        if fields.kind() != "ordered_field_declaration_list" {
            return None;
        }
        let mut field_cursor = fields.walk();
        let field_type = fields.children_by_field_name("type", &mut field_cursor).nth(index)?;
        return field_type.utf8_text(source).ok().map(|t| t.trim().to_string());
    }
    None
}

/// If `cast`'s operand is an index expression `base[idx]` whose `base` resolves to
/// a slice/array/Vec/Box-slice of a fixed-width integer, return that element
/// type's name (`"u8"`, `"i32"`, …); otherwise `None`.
///
/// This lets the numeric-cast rules resolve the source type of the idiomatic
/// byte-buffer assembly `(buf[0] as u32) | (buf[1] as u32) << 8 | …` where
/// `buf: &[u8; N]`: `buf[0]` is `u8`, so the cast to a wider integer is a
/// provable widening. Without this, the index operand resolves to no source type
/// and both rules flag it conservatively.
///
/// Two base shapes are resolved:
///
/// - a bare `identifier` whose local binding either is annotated with one of the
///   recognized container shapes of a known fixed-width integer (`buf: &[u8; N]`)
///   or is a `let` initialized by an `.as_bytes()` call (`let bytes =
///   s.as_bytes();`) — `str`/`String`/`OsStr`/`CStr::as_bytes` all return
///   `&[u8]`, so `bytes[i]` is `u8`;
/// - a direct `.as_bytes()` call (`s.as_bytes()[i]`), likewise `u8`.
///
/// Resolution is otherwise narrow — a base from any other method return, an
/// un-annotated binding of an unknown shape, or a non-integer element type yields
/// `None`, so genuinely unresolvable casts stay flagged.
///
/// The returned type name is fed back through each rule's own width/signedness
/// table, so signedness and the widening/narrowing decision are not re-derived
/// here. Shared by `rust-no-as-numeric-cast` and `rust-no-lossy-as-cast`.
pub fn cast_operand_indexed_element_type(cast: Node, source: &[u8]) -> Option<String> {
    let value = cast.child_by_field_name("value")?;
    if value.kind() != "index_expression" {
        return None;
    }
    // tree-sitter exposes the indexed container as the first named child; the
    // remaining children are the bracket tokens and the index expression.
    let base = value.named_child(0)?;
    match base.kind() {
        // `s.as_bytes()[i]` — the element of a `&[u8]` is `u8`.
        "call_expression" if call_is_as_bytes(base, source) => Some("u8".to_string()),
        "identifier" => {
            let name = base.utf8_text(source).ok()?;
            // A binding annotated with a recognized integer-container type.
            if let Some(declared) = find_identifier_type(cast, name, source) {
                return element_int_type(&declared).map(str::to_string);
            }
            // A binding initialized by `.as_bytes()` (`let bytes = s.as_bytes();`),
            // whose type is inferred rather than annotated.
            local_binding_is_as_bytes(cast, name, source).then(|| "u8".to_string())
        }
        _ => None,
    }
}

/// True when `call` is a zero-argument `.as_bytes()` method call (the receiver is
/// not inspected — `str`, `String`, `OsStr`, and `CStr` all return `&[u8]`).
fn call_is_as_bytes(call: Node, source: &[u8]) -> bool {
    if call
        .child_by_field_name("arguments")
        .is_some_and(|args| args.named_child_count() > 0)
    {
        return false;
    }
    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    function.kind() == "field_expression"
        && function
            .child_by_field_name("field")
            .and_then(|field| field.utf8_text(source).ok())
            == Some("as_bytes")
}

/// True when `name` resolves to a local `let` binding (in scope before `node`)
/// whose initializer is an `.as_bytes()` call, so the binding holds a `&[u8]`.
fn local_binding_is_as_bytes(node: Node, name: &str, source: &[u8]) -> bool {
    let mut current = Some(node);
    while let Some(n) = current {
        if matches!(
            n.kind(),
            "function_item" | "closure_expression" | "block" | "source_file"
        ) && find_as_bytes_binding_before(n, node.start_byte(), name, source)
        {
            return true;
        }
        current = n.parent();
    }
    false
}

/// Walk `node`'s subtree (positions before `limit`) for a `let <name> =
/// <expr>.as_bytes();` declaration.
fn find_as_bytes_binding_before(node: Node, limit: usize, name: &str, source: &[u8]) -> bool {
    if node.start_byte() >= limit {
        return false;
    }
    if node.kind() == "let_declaration"
        && node
            .child_by_field_name("pattern")
            .is_some_and(|pattern| pattern_contains_identifier(pattern, name, source))
        && node.child_by_field_name("value").is_some_and(|value| {
            value.kind() == "call_expression" && call_is_as_bytes(value, source)
        })
    {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| find_as_bytes_binding_before(child, limit, name, source))
}

/// Extract the element type name from a container type that is a slice, array,
/// `Vec<T>`, or `Box<[T]>` of a fixed-width integer, returning `None` for any
/// other shape or a non-integer element.
///
/// Leading `&` / `&mut` reference markers are stripped first, so `&[u8]`,
/// `&mut [u8]`, `[u8]`, `[u8; 8]`, `Vec<u8>`, and `Box<[u8]>` all resolve to
/// `"u8"`. The element must be one of the fixed-width integer primitives
/// (`u8`..`u128`, `i8`..`i128`); `usize`/`isize` (platform width) and any
/// non-integer element return `None`.
fn element_int_type(declared: &str) -> Option<&str> {
    let mut t = declared.trim();
    // Strip leading reference markers: `&`, `&mut `, possibly repeated.
    loop {
        t = t.trim();
        if let Some(rest) = t.strip_prefix('&') {
            t = rest.trim_start();
            if let Some(rest) = t.strip_prefix("mut ") {
                t = rest;
            }
        } else {
            break;
        }
    }
    let element = if let Some(inner) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        // Slice `[T]` or array `[T; N]`.
        inner.split(';').next().unwrap_or(inner).trim()
    } else if let Some(inner) = t.strip_prefix("Vec<").and_then(|s| s.strip_suffix('>')) {
        inner.trim()
    } else if let Some(inner) = t.strip_prefix("Box<").and_then(|s| s.strip_suffix('>')) {
        // `Box<[T]>` — the box wraps a slice.
        let slice = inner.trim();
        slice
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))?
            .trim()
    } else {
        return None;
    };
    is_fixed_width_int(element).then_some(element)
}

/// True if `name` is one of the fixed-width integer primitives — `u8`..`u128`,
/// `i8`..`i128`. Excludes `usize`/`isize`, whose width is platform-dependent.
fn is_fixed_width_int(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16" | "u32" | "u64" | "u128" | "i8" | "i16" | "i32" | "i64" | "i128"
    )
}

/// True if `cast` (a `type_cast_expression`) casts a boolean-producing operand
/// to an integer. `bool as <integer>` is always lossless and total
/// (`false` → 0, `true` → 1; a `bool` is a single bit that fits every integer
/// target), so suggesting `try_into()` there only manufactures an error path
/// that can never be reached.
///
/// The operand (the `value` field of the cast) is recognized as boolean when it
/// is one of:
/// - a `boolean_literal` (`true` / `false`);
/// - a `binary_expression` with a comparison (`==`, `!=`, `<`, `<=`, `>`, `>=`)
///   or logical (`&&`, `||`) operator — these always yield `bool`;
/// - a `unary_expression` `!<operand>` whose operand is itself boolean (`!` on
///   an integer is bitwise NOT and stays integer, so the operand is checked
///   recursively);
/// - a `parenthesized_expression` wrapping any of the above (peeled, so
///   `(3 > 2) as u8` is covered);
/// - a `call_expression` proven `bool` either by the bool-returning method-name
///   convention — an `is_`/`has_` prefix, or exactly `contains`, `starts_with`,
///   or `ends_with` (covers `value.is_some() as u8`) — or by resolving the callee
///   (a method, free function, or path) to a `function_item` defined in the same
///   file whose declared return type is `bool` (covers `self.get_random_bit() as
///   u8` where `fn get_random_bit(&self) -> bool`);
/// - a bare `identifier` whose local binding is annotated `bool` (`b as u8`).
///
/// The method-name set is a deliberately narrow heuristic; it must not be
/// broadened, since an arbitrary method may return any integer type. The
/// same-file signature resolution is the safe generalization: a call whose
/// callee cannot be resolved to a `-> bool` definition in the file stays flagged.
///
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`, which both
/// otherwise flag `bool as u8` because the operand type is not resolved from the
/// AST.
pub fn cast_operand_is_bool(cast: Node, source: &[u8]) -> bool {
    let Some(value) = cast.child_by_field_name("value") else {
        return false;
    };
    operand_is_bool(value, source)
}

/// True when `node` is provably `bool` from its syntactic shape (or a bool-typed
/// local binding / bool-returning method call). Used to recognise both the
/// `bool as integer` cast idiom and the branchless `bool & bool` / `bool | bool`
/// (non-short-circuit logical) idiom.
pub fn operand_is_bool(node: Node, source: &[u8]) -> bool {
    match node.kind() {
        "boolean_literal" => true,
        "parenthesized_expression" => node
            .named_child(0)
            .is_some_and(|inner| operand_is_bool(inner, source)),
        "binary_expression" => node
            .child_by_field_name("operator")
            .and_then(|op| op.utf8_text(source).ok())
            .is_some_and(|op| {
                matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||")
            }),
        "unary_expression" => {
            // `!` is logical NOT only when its operand is bool; on an integer it
            // is bitwise NOT and stays integer, so recurse into the operand.
            let is_not = node
                .child(0)
                .and_then(|op| op.utf8_text(source).ok())
                .is_some_and(|op| op == "!");
            is_not
                && node
                    .named_child(0)
                    .is_some_and(|operand| operand_is_bool(operand, source))
        }
        "call_expression" => call_returns_bool(node, source),
        "identifier" => node.utf8_text(source).ok().is_some_and(|name| {
            // A binding explicitly annotated `bool`, or — for an inferred
            // binding — one whose initializer is itself provably bool
            // (`let enabled = lo <= x;`), so `enabled as u32` is a bool cast.
            find_identifier_type(node, name, source).as_deref() == Some("bool")
                || local_binding_initializer_is_bool(node, name, source)
        }),
        _ => false,
    }
}

/// True when `name`'s nearest in-scope `let` binding before `node` has an
/// initializer that is itself provably bool (`let enabled = lo <= x;`). This
/// resolves the inferred-bool local that `find_identifier_type` misses — it only
/// reads explicit `bool` annotations, and bool comparison/logical bindings are
/// written without one. A parameter or annotated binding is handled by the
/// annotation path; this covers only `let <name> = <bool-expr>;`.
///
/// Scopes are walked innermost-first and the *nearest* preceding binding decides:
/// a later non-bool shadow (`let n = a < b; let n = a + b; n as u32`) is not
/// bool, and a sibling/inner-block binding that does not enclose `node` is
/// ignored. The matched binding's own initializer is judged with the binding's
/// position as the lookup limit, so a self-referential `let x = x;` cannot
/// resolve to itself (which would recurse forever).
fn local_binding_initializer_is_bool(node: Node, name: &str, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if matches!(
            n.kind(),
            "function_item" | "closure_expression" | "block" | "source_file"
        ) && let Some(value) = nearest_binding_value_before(n, node.start_byte(), name, source)
        {
            // The nearest binding in this scope is authoritative — its
            // initializer alone decides, even when it is not bool (shadowing).
            return operand_is_bool(value, source);
        }
        current = n.parent();
    }
    false
}

/// The initializer expression of the nearest `let <name> = <expr>;` declared
/// directly within `scope` (a single statement-bearing block, not its nested
/// child blocks — those bindings do not reach `limit`) whose initializer ends
/// before `limit`. Returns the value node of the latest such binding, so the
/// nearest shadow wins. Requiring `value.end_byte() <= limit` excludes a
/// self/forward reference (`let x = x;` resolving its own RHS), which would
/// otherwise re-enter the same binding and recurse without progress. `None` when
/// the scope declares no such binding before `limit`.
fn nearest_binding_value_before<'a>(
    scope: Node<'a>,
    limit: usize,
    name: &str,
    source: &[u8],
) -> Option<Node<'a>> {
    let mut best: Option<Node<'a>> = None;
    let mut cursor = scope.walk();
    for child in scope.named_children(&mut cursor) {
        if child.start_byte() >= limit {
            break;
        }
        if child.kind() != "let_declaration" {
            continue;
        }
        let Some(value) = child.child_by_field_name("value") else {
            continue;
        };
        if value.end_byte() <= limit
            && child
                .child_by_field_name("pattern")
                .is_some_and(|pattern| pattern_contains_identifier(pattern, name, source))
        {
            best = Some(value);
        }
    }
    best
}

/// True if `call` is a call expression whose result is provably `bool`, by either
/// the bool-returning method-name convention or a same-file function definition
/// whose declared return type is `bool`.
///
/// The name convention recognizes a method call (`<receiver>.method(...)`) whose
/// method name has an `is_`/`has_` prefix, or is exactly `contains` /
/// `starts_with` / `ends_with`. When the name does not match, the callee — a
/// method (`self.get_random_bit()`), a free function (`f()`), or a path
/// (`module::f()`) — is resolved to a `function_item` defined in the same file
/// whose `return_type` is `bool`.
fn call_returns_bool(call: Node, source: &[u8]) -> bool {
    const BOOL_METHODS: &[&str] = &["contains", "starts_with", "ends_with"];

    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    let Some(callee_name) = callee_name(function, source) else {
        return false;
    };
    if callee_name.starts_with("is_") || callee_name.starts_with("has_") {
        return true;
    }
    if function.kind() == "field_expression" && BOOL_METHODS.contains(&callee_name) {
        return true;
    }
    // No name convention matched — resolve the callee to a same-file function
    // definition and check its declared return type. An out-of-file callee or an
    // unresolved name yields `None`, so the cast stays conservatively flagged.
    fn_returns_bool_in_file(call, callee_name, source)
}

/// The bare name of a call's callee: the `field` of a method call's
/// `field_expression` (`self.get_random_bit` → `get_random_bit`), the last
/// segment of a `scoped_identifier` path (`module::f` → `f`), or a plain
/// `identifier` (`f` → `f`). Other callee shapes yield `None`.
fn callee_name<'a>(function: Node, source: &'a [u8]) -> Option<&'a str> {
    let name_node = match function.kind() {
        "field_expression" => function.child_by_field_name("field")?,
        "scoped_identifier" => function.child_by_field_name("name")?,
        "identifier" => function,
        _ => return None,
    };
    name_node.utf8_text(source).ok()
}

/// True if a `function_item` named `name` defined anywhere in the same file has a
/// `return_type` of exactly `bool`. Walks to the `source_file` root and scans all
/// descendant function items (free functions and `impl`/`trait` methods). The
/// match is by name only — same-file resolution does not model receiver types, so
/// a name collision across `impl` blocks would conservatively report the first
/// `bool`-returning definition; an unresolved name keeps the cast flagged.
fn fn_returns_bool_in_file(node: Node, name: &str, source: &[u8]) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    fn_with_name_returns_bool(root, name, source)
}

/// Recursively search `node`'s subtree for a `function_item` named `name` whose
/// `return_type` is `bool`.
fn fn_with_name_returns_bool(node: Node, name: &str, source: &[u8]) -> bool {
    if node.kind() == "function_item"
        && node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            == Some(name)
        && node
            .child_by_field_name("return_type")
            .and_then(|rt| rt.utf8_text(source).ok())
            .is_some_and(|rt| rt.trim() == "bool")
    {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| fn_with_name_returns_bool(child, name, source))
}

/// True when the operand of `cast` (a `type_cast_expression`) is a `char`: a
/// `char_literal` (`'A' as u32`), an identifier whose local binding is annotated
/// `char` (`c as u32`), an identifier bound by a `chars()`/`char_indices()`
/// for-loop (`for c in s.chars()`), an identifier bound by `if let Some(c) =
/// char::from_u32(..)` / `while let Some(c) = char::from_digit(..)` (a
/// `Some`-unwrap of an `Option<char>`-returning function), or a dereference of a
/// `&char` range accessor (`*range.start() as u32` where `range:
/// RangeInclusive<char>`).
///
/// A `char` is a Unicode scalar value in `0..=0x10FFFF` (21 bits), so casting it
/// to any integer at least 21 bits wide is lossless and total. Shared by
/// `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`, which both otherwise
/// flag the cast because a `char` operand resolves to no numeric source type.
pub fn cast_operand_is_char(cast: Node, source: &[u8]) -> bool {
    let Some(value) = cast.child_by_field_name("value") else {
        return false;
    };
    operand_is_char(value, cast, source)
}

/// Names of zero-argument inherent methods that, on a `char`-parameterized range,
/// return `&char` — so dereferencing the call yields a `char`. Restricted to the
/// inherent `RangeInclusive` accessors (`range.start()` / `range.end()`), where
/// the deref-then-widening-cast shape is a strong char signal.
const CHAR_REF_RANGE_ACCESSORS: &[&str] = &["start", "end"];

/// True when `node` is provably a `char` value:
/// - a `char_literal`;
/// - a `parenthesized_expression` wrapping a char (peeled);
/// - a `unary_expression` `*<expr>` dereferencing a `&char` range accessor call
///   (`*range.start()`), since `*&char` is `char`;
/// - an `identifier` whose local binding is annotated `char`, or bound by a
///   `chars()`/`char_indices()` for-loop.
///
/// `cast` is the enclosing `type_cast_expression`, used as the lookup anchor for
/// identifier-binding resolution.
fn operand_is_char(node: Node, cast: Node, source: &[u8]) -> bool {
    match node.kind() {
        "char_literal" => true,
        "parenthesized_expression" => node
            .named_child(0)
            .is_some_and(|inner| operand_is_char(inner, cast, source)),
        "unary_expression" => {
            // `*<call>` is `char` only when `<call>` returns `&char`; the deref
            // operator is the first anonymous child of the unary expression.
            let is_deref = node
                .child(0)
                .and_then(|op| op.utf8_text(source).ok())
                .is_some_and(|op| op == "*");
            is_deref
                && node
                    .named_child(0)
                    .is_some_and(|operand| call_returns_char_ref(operand, source))
        }
        "identifier" => node.utf8_text(source).ok().is_some_and(|name| {
            find_identifier_type(cast, name, source)
                .is_some_and(|type_text| type_text == "char")
                || binding_is_chars_iter(cast, name, source)
                || binding_is_char_option_unwrap(cast, name, source)
        }),
        _ => false,
    }
}

/// True when `node` is a no-argument method call `<receiver>.<method>()` whose
/// method name is a `char`-range accessor returning `&char` (`range.start()` /
/// `range.end()`). The receiver type is not provable from the AST, so the
/// method-name set is deliberately narrow to stay sound.
fn call_returns_char_ref(node: Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    if node
        .child_by_field_name("arguments")
        .is_some_and(|args| args.named_child_count() > 0)
    {
        return false;
    }
    let Some(function) = node.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    function
        .child_by_field_name("field")
        .and_then(|field| field.utf8_text(source).ok())
        .is_some_and(|name| CHAR_REF_RANGE_ACCESSORS.contains(&name))
}

/// True when `name` is the `char` binding of an enclosing `if let Some(name) =
/// char::from_u32(..)` or `while let Some(name) = char::from_digit(..)` — a
/// `Some`-unwrap of a call to a well-known `Option<char>`-returning function.
///
/// `char::from_u32` and `char::from_digit` return `Option<char>`, so the
/// `Some(name)` pattern binds `name: char` unconditionally. The match requires
/// both the call's final path segment to be in the closed char-returning set and
/// a `char` segment in the path, so an unrelated `from_u32` on another type does
/// not qualify.
fn binding_is_char_option_unwrap(node: Node, name: &str, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if matches!(n.kind(), "if_expression" | "while_expression")
            && let Some(condition) = n.child_by_field_name("condition")
            && condition.kind() == "let_condition"
            && let Some(pattern) = condition.child_by_field_name("pattern")
            && some_pattern_binds(pattern, name, source)
            && let Some(value) = condition.child_by_field_name("value")
            && call_returns_char_option(value, source)
        {
            return true;
        }
        current = n.parent();
    }
    false
}

/// True when `pattern` is `Some(name)` — a `tuple_struct_pattern` whose type
/// path is `Some` wrapping the single identifier binding `name`.
fn some_pattern_binds(pattern: Node, name: &str, source: &[u8]) -> bool {
    if pattern.kind() != "tuple_struct_pattern" {
        return false;
    }
    let is_some = pattern
        .child_by_field_name("type")
        .and_then(|t| t.utf8_text(source).ok())
        .is_some_and(|t| t == "Some");
    is_some
        && pattern.named_child_count() == 2
        && pattern.named_child(1).is_some_and(|binding| {
            binding.kind() == "identifier" && binding.utf8_text(source).is_ok_and(|t| t == name)
        })
}

/// True when `call` is a `<path>(..)` whose path resolves to a well-known
/// `Option<char>`-returning function on the `char` primitive: `char::from_u32`
/// or `char::from_digit`, optionally module-qualified as `std::char::…` /
/// `core::char::…` (with or without a leading `::`).
///
/// The path is matched against a closed set rather than a `…::char` suffix so a
/// user module literally named `char` exposing a `from_u32`/`from_digit` does
/// not qualify.
fn call_returns_char_option(call: Node, source: &[u8]) -> bool {
    const CHAR_OPTION_FNS: &[&str] = &["from_u32", "from_digit"];
    const CHAR_PATHS: &[&str] = &[
        "char",
        "std::char",
        "core::char",
        "::std::char",
        "::core::char",
    ];

    if call.kind() != "call_expression" {
        return false;
    }
    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "scoped_identifier" {
        return false;
    }
    let last = function
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok());
    let path = function
        .child_by_field_name("path")
        .and_then(|p| p.utf8_text(source).ok());
    last.is_some_and(|name| CHAR_OPTION_FNS.contains(&name))
        && path.is_some_and(|p| CHAR_PATHS.contains(&p))
}

/// True when `name` is the `char` binding of an enclosing `for <pat> in
/// <expr>.chars()` or `for (<idx>, <name>) in <expr>.char_indices()` loop.
///
/// `<str>.chars()` yields `char`, and `<str>.char_indices()` yields `(usize,
/// char)` — so the plain loop binding (or the tuple's second element) is a
/// `char`. The match is on the iterator's method name, not the receiver, since
/// any `&str`/`String` chain ending in those inherent methods yields a `char`.
fn binding_is_chars_iter(node: Node, name: &str, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "for_expression"
            && let Some(pattern) = n.child_by_field_name("pattern")
            && for_pattern_binds_char(pattern, name, source)
            && let Some(value) = n.child_by_field_name("value")
            && let Some(method) = chars_iter_method(value, source)
        {
            return (method == "chars" && pattern.kind() == "identifier")
                || (method == "char_indices" && pattern.kind() == "tuple_pattern");
        }
        current = n.parent();
    }
    false
}

/// True when `pattern` is the for-loop binding site for the `char` value of a
/// `chars()`/`char_indices()` iterator: either the plain identifier `name`
/// (`for name in ...chars()`), or the second element of a two-element tuple
/// pattern (`for (_, name) in ...char_indices()`).
fn for_pattern_binds_char(pattern: Node, name: &str, source: &[u8]) -> bool {
    match pattern.kind() {
        "identifier" => pattern.utf8_text(source).is_ok_and(|text| text == name),
        "tuple_pattern" => {
            pattern.named_child_count() == 2
                && pattern.named_child(1).is_some_and(|second| {
                    second.kind() == "identifier"
                        && second.utf8_text(source).is_ok_and(|text| text == name)
                })
        }
        _ => false,
    }
}

/// The method name of a no-argument `<expr>.<method>()` call, or `None` if the
/// node is not such a method call.
fn chars_iter_method<'a>(value: Node, source: &'a [u8]) -> Option<&'a str> {
    if value.kind() != "call_expression" {
        return None;
    }
    if value
        .child_by_field_name("arguments")
        .is_some_and(|args| args.named_child_count() > 0)
    {
        return None;
    }
    let function = value.child_by_field_name("function")?;
    if function.kind() != "field_expression" {
        return None;
    }
    function
        .child_by_field_name("field")
        .and_then(|field| field.utf8_text(source).ok())
}

/// `char` inspection methods that prove their receiver is an ASCII character
/// (Unicode scalar value `0x00..=0x7F`). `char::is_ascii` is the general check;
/// every `is_ascii_*` variant tests a subset of ASCII, so each equally proves
/// the value is in `0..=127`.
const CHAR_IS_ASCII_PREDICATES: &[&str] = &[
    "is_ascii",
    "is_ascii_alphabetic",
    "is_ascii_alphanumeric",
    "is_ascii_control",
    "is_ascii_digit",
    "is_ascii_graphic",
    "is_ascii_hexdigit",
    "is_ascii_lowercase",
    "is_ascii_punctuation",
    "is_ascii_uppercase",
    "is_ascii_whitespace",
];

/// True if `cast` (a `type_cast_expression`) casts a `char` to an integer in a
/// position dominated by an `is_ascii()` check on the SAME value, so the cast is
/// provably lossless — e.g. `self.is_ascii().then_some(*self as u8)` or
/// `if ch.is_ascii() { ch as u8 }`.
///
/// An ASCII `char` is `0x00..=0x7F`, which fits every integer at least 8 bits
/// wide (`u8`..`u128`, `i8`..`i128`). Outside the guard a `char as u8` truncates
/// the upper bits, so the rules flag it; under the guard the value is provably in
/// range and `try_from` there manufactures an unreachable error path.
///
/// The exemption is deliberately narrow to stay sound — every condition must
/// hold:
///
/// - the cast operand is a single value reference: a bare `identifier` (`ch`) or
///   a dereference of one (`*self`), optionally parenthesized;
/// - a dominating guard tests `is_ascii` / `is_ascii_*` on that SAME value,
///   reached either through `<value>.is_ascii().then_some(<cast>)` /
///   `.then(|| <cast>)` (the cast is the argument of the `then_some`/`then`
///   whose receiver is the `is_ascii` call), or through the `consequence` of an
///   enclosing `if <value>.is_ascii() { … <cast> … }` (never the `else` branch,
///   which is the negation).
///
/// The guard is matched by method name (the AST carries no receiver type), the
/// same name-based soundness tolerance the sibling `is_positive`/`is_negative`
/// guards already accept: a user type with a custom `is_ascii` not proving
/// `0..=127` would be exempted, an accepted false negative.
///
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`, which both
/// otherwise flag the cast because a `char as u8` is lossy in general.
pub fn cast_operand_is_ascii_guarded(cast: Node, source: &[u8]) -> bool {
    let Some(operand) = cast.child_by_field_name("value") else {
        return false;
    };
    let Some(operand_text) = strip_deref_and_parens(operand, source) else {
        return false;
    };
    enclosing_ascii_guard_matches(cast, operand_text, source)
}

/// The text of `node` with any leading dereferences (`*x`) and surrounding
/// parentheses peeled off, e.g. `(*self)` → `self`. Returns `None` unless the
/// peeled node is a bare `identifier` or `self`, so only a single named value
/// qualifies as the guarded operand.
fn strip_deref_and_parens<'a>(node: Node, source: &'a [u8]) -> Option<&'a str> {
    let mut current = node;
    loop {
        match current.kind() {
            "identifier" | "self" => return current.utf8_text(source).ok(),
            "parenthesized_expression" => current = current.named_child(0)?,
            "unary_expression" => {
                let is_deref = current
                    .child(0)
                    .and_then(|op| op.utf8_text(source).ok())
                    .is_some_and(|op| op == "*");
                if !is_deref {
                    return None;
                }
                current = current.named_child(0)?;
            }
            _ => return None,
        }
    }
}

/// True if a guard dominating `cast` tests `is_ascii`/`is_ascii_*` on the value
/// named `operand`. Ascends via `parent()`, recognizing two guard sites:
///
/// - `<operand>.is_ascii().then_some(<cast>)` / `.then(|| <cast>)` — the cast
///   sits in the `arguments` of a `then_some`/`then` call whose receiver is the
///   `is_ascii` call;
/// - `if <operand>.is_ascii() { … <cast> … }` — the cast is reached through the
///   `if`'s `consequence`.
///
/// The walk stops at the enclosing `function_item` boundary. A `closure_expression`
/// also stops it unless the closure is the direct argument of a `then`/`then_some`
/// ascii-guard call (`.then(|| <cast>)`): a guard cannot prove anything about a
/// closure invoked elsewhere, but `then`/`then_some` runs its closure only when
/// the predicate held.
fn enclosing_ascii_guard_matches(cast: Node, operand: &str, source: &[u8]) -> bool {
    let mut child = cast;
    while let Some(parent) = child.parent() {
        match parent.kind() {
            "function_item" => return false,
            "closure_expression" => {
                // The closure body is in range only when the closure itself is
                // the `then`/`then_some` arm; otherwise the guard does not reach
                // the closure's invocation site.
                if !closure_is_ascii_then_arm(parent, operand, source) {
                    return false;
                }
            }
            "call_expression" => {
                if call_is_ascii_then_guard(parent, operand, source) {
                    return true;
                }
            }
            "if_expression" => {
                if parent.child_by_field_name("consequence") == Some(child)
                    && let Some(condition) = parent.child_by_field_name("condition")
                    && expr_is_ascii_check(condition, operand, source)
                {
                    return true;
                }
            }
            _ => {}
        }
        child = parent;
    }
    false
}

/// True if `closure` is the direct argument of a `then`/`then_some` call whose
/// receiver is an `is_ascii`/`is_ascii_*` check on `operand` — the
/// `<operand>.is_ascii().then(|| …)` arm.
fn closure_is_ascii_then_arm(closure: Node, operand: &str, source: &[u8]) -> bool {
    closure
        .parent()
        .filter(|args| args.kind() == "arguments")
        .and_then(|args| args.parent())
        .filter(|call| call.kind() == "call_expression")
        .is_some_and(|call| call_is_ascii_then_guard(call, operand, source))
}

/// True if `call` is `<operand>.is_ascii().then_some(..)` or
/// `<operand>.is_ascii().then(..)` — a `then_some`/`then` call whose receiver is
/// an `is_ascii`/`is_ascii_*` check on `operand`. The cast lives in the call's
/// arguments, so the receiver's guard dominates it.
fn call_is_ascii_then_guard(call: Node, operand: &str, source: &[u8]) -> bool {
    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    let is_then = function
        .child_by_field_name("field")
        .and_then(|f| f.utf8_text(source).ok())
        .is_some_and(|f| f == "then_some" || f == "then");
    if !is_then {
        return false;
    }
    function
        .child_by_field_name("value")
        .is_some_and(|recv| expr_is_ascii_check(recv, operand, source))
}

/// True if `expr` is a no-argument method call `<operand>.is_ascii()` (or any
/// `is_ascii_*` variant) on the value named `operand`, after peeling any deref
/// or parentheses from the receiver (`(*self).is_ascii()` counts).
fn expr_is_ascii_check(expr: Node, operand: &str, source: &[u8]) -> bool {
    if expr.kind() != "call_expression" {
        return false;
    }
    if expr
        .child_by_field_name("arguments")
        .is_some_and(|args| args.named_child_count() > 0)
    {
        return false;
    }
    let Some(function) = expr.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    let method_is_ascii = function
        .child_by_field_name("field")
        .and_then(|f| f.utf8_text(source).ok())
        .is_some_and(|f| CHAR_IS_ASCII_PREDICATES.contains(&f));
    method_is_ascii
        && function
            .child_by_field_name("value")
            .and_then(|recv| strip_deref_and_parens(recv, source))
            == Some(operand)
}

/// True if `cast` (a `type_cast_expression`) narrows an identifier whose value
/// an enclosing `if` guard or a preceding early-exit guard proves fits the
/// unsigned target type, so the `as`-cast cannot overflow — e.g.
/// `if val < 256 { val as u8 }` or `if val > 100 { return Err(…) } else { val as u8 }`.
///
/// This is the canonical "pick the smallest representation that fits" encoder
/// pattern: each branch is entered only when the value is within the target
/// type's range, so `try_from` there manufactures an unreachable error path.
///
/// The exemption is deliberately narrow to stay sound — every condition must
/// hold:
///
/// - the operand is a bare `identifier` (`val`), not an expression;
/// - the target is an **unsigned** integer (`u8`..`u128`/`usize`);
/// - the operand's source type resolves from the AST to an **unsigned** integer,
///   which proves the value is non-negative (lower bound 0). An unresolved or
///   signed source is not exempted: an upper-bound guard alone cannot rule out a
///   negative value wrapping on the cast;
/// - the value is proven to fit by one of three dominating bounds, each on the
///   SAME identifier against an integer literal `N`:
///   - an enclosing `if_expression` reached through its `consequence` whose
///     condition directly upper-bounds the value — `val < N` with `N <= 2^bits`,
///     or `val <= N` with `N <= 2^bits - 1` (the symmetric `N > val` / `N >= val`
///     forms count too);
///   - an enclosing `if_expression` reached through its `alternative` (the `else`
///     / `else if` branch), which is entered only when the condition is false, so
///     the condition's negation upper-bounds the value — `val > N` (proves
///     `val <= N`, fits when `N <= 2^bits - 1`) or `val >= N` (proves
///     `val <= N - 1`, fits when `N - 1 <= 2^bits - 1`), and the mirrored
///     `N < val` / `N <= val`;
///   - a preceding early-exit guard `if val > N { … }` in the same block, with no
///     `else`, whose `then` branch unconditionally diverges (`return` / `break` /
///     `continue` / `panic!` / `unreachable!` / `todo!` / `unimplemented!`), so the
///     cast is reached only when the condition is false (`val <= N`), with the same
///     fit requirement as the `else` form;
/// - the identifier is not re-bound (a shadowing `let val`) or reassigned
///   (`val = …` / `val += …`) between the guard and the cast, which would break
///   the link between the guard's bound and the value the cast reads.
///
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`, which both
/// otherwise flag the narrowing because the operand's bounded range is not
/// visible from the cast in isolation.
pub fn cast_operand_is_range_guarded(cast: Node, source: &[u8]) -> bool {
    let Some(value) = cast.child_by_field_name("value") else {
        return false;
    };
    if value.kind() != "identifier" {
        return false;
    }
    let Ok(name) = value.utf8_text(source) else {
        return false;
    };
    // Target must be an unsigned integer; capture its bit width.
    let Some(target_bits) = cast
        .child_by_field_name("type")
        .and_then(|t| t.utf8_text(source).ok())
        .and_then(unsigned_int_bits)
    else {
        return false;
    };
    // The source must resolve to an unsigned integer so the value is provably
    // non-negative; an upper-bound guard cannot otherwise rule out underflow.
    if find_identifier_type(cast, name, source)
        .and_then(|t| unsigned_int_bits(&t))
        .is_none()
    {
        return false;
    }
    enclosing_upper_bound_fits(cast, name, target_bits, source)
        || preceding_exit_guard_upper_bounds(cast, name, target_bits, source)
}

/// True if `cast` (a `type_cast_expression`) narrows the induction variable of an
/// enclosing `for <name> in <range>` loop whose range's integer-literal bounds
/// prove every value the variable takes fits the cast's target type, so the
/// `as`-cast cannot truncate or change sign — e.g. `for n in 0usize..256 { n as
/// u32 }` (`n ∈ [0, 255] ⊂ u32`) or `for n in 0u32..=1000 { n as i32 }`
/// (`n ∈ [0, 1000] ⊂ i32`).
///
/// A `for` loop over a `range_expression` binds its variable to every value of a
/// statically known integer interval, exactly the way an enclosing `if val < N`
/// guard bounds a value — but the range proves BOTH ends, so a signed target is in
/// scope too (unlike [`cast_operand_is_range_guarded`], which needs a separate
/// non-negativity proof for the lower bound).
///
/// The exemption is deliberately narrow to stay sound — every condition must hold:
///
/// - the operand is a bare `identifier` (`n`), not an expression;
/// - the target is an integer type (`u8`..`u128`/`usize` or `i8`..`i128`/`isize`);
/// - the nearest enclosing binding of `name` is a `for_expression` whose `pattern`
///   is exactly that identifier and whose iterator is a `range_expression` with two
///   integer-literal (or `const`-resolved) bounds: `a..b` gives `[a, b - 1]`,
///   `a..=b` gives `[a, b]`. A non-literal bound, a half-open range, or a `...`
///   operator yields no interval, leaving the cast flagged;
/// - the whole interval `[lo, hi]` is representable in the target type
///   ([`interval_fits_int_target`]): `for n in 0..=256 { n as u8 }` stays flagged
///   (`256 > u8::MAX`), as does `for n in -5..10 { n as u8 }` (`lo < 0` into an
///   unsigned target);
/// - `name` is not re-bound (a shadowing `let n`) or reassigned between the loop
///   body's start and the cast, which would break the link between the range and
///   the value the cast reads.
///
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`, which both
/// otherwise flag the narrowing because the induction variable's bounded range is
/// not visible from the cast in isolation.
pub fn cast_operand_is_for_range_bounded(cast: Node, source: &[u8]) -> bool {
    let Some(value) = cast.child_by_field_name("value") else {
        return false;
    };
    if value.kind() != "identifier" {
        return false;
    }
    let Ok(name) = value.utf8_text(source) else {
        return false;
    };
    // Target must be an integer type (signed or unsigned); the range proves both
    // ends, so either signedness is in scope.
    let Some(target) = cast
        .child_by_field_name("type")
        .and_then(|t| t.utf8_text(source).ok())
        .map(str::trim)
    else {
        return false;
    };
    if unsigned_int_bits(target).is_none() && signed_int_bits(target).is_none() {
        return false;
    }
    // Ascend to the nearest enclosing binding of `name`. A `for_expression` whose
    // pattern binds it is THE induction binding; any other binder (an inner
    // shadowing `for`/closure pattern, a `match_arm` / `if let` / `while let`
    // pattern crossed on the way up) makes the range inapplicable. The walk stops
    // at the enclosing function / closure boundary.
    let cast_start = cast.start_byte();
    let mut child = cast;
    while let Some(parent) = child.parent() {
        match parent.kind() {
            "function_item" | "closure_expression" => return false,
            "for_expression"
                if parent
                    .child_by_field_name("pattern")
                    .is_some_and(|p| pattern_contains_identifier(p, name, source)) =>
            {
                return for_loop_range_fits_target(parent, name, target, cast_start, source);
            }
            // A pattern binding of `name` introduced by an intervening scope — a
            // `match_arm` pattern, or an `if let` / `while let` `let_condition` —
            // shadows the induction variable: the cast reads that binding, not the
            // loop var, so the range no longer applies.
            _ if intervening_pattern_binds_name(parent, child, name, source) => return false,
            _ => {}
        }
        child = parent;
    }
    false
}

/// True if ascending from `child` into `parent` crosses a *pattern* binding of
/// `name` that shadows an outer loop variable: a `match_arm` whose pattern binds
/// `name` and whose `value` body contains the cast, or an `if`/`while` expression
/// reached through its guarded body whose condition is (or contains) a
/// `let_condition` binding `name` (`if let Some(n) = …`). Such a binding rebinds
/// `name` to an unrelated value, so a `for`-range exemption above it is unsound.
fn intervening_pattern_binds_name(parent: Node, child: Node, name: &str, source: &[u8]) -> bool {
    match parent.kind() {
        "match_arm" => {
            parent.child_by_field_name("value") == Some(child)
                && parent
                    .child_by_field_name("pattern")
                    .is_some_and(|p| pattern_contains_identifier(p, name, source))
        }
        "if_expression" | "while_expression" => {
            let entered_body = parent.child_by_field_name("consequence") == Some(child)
                || parent.child_by_field_name("body") == Some(child);
            entered_body
                && parent
                    .child_by_field_name("condition")
                    .is_some_and(|c| condition_let_binds_name(c, name, source))
        }
        _ => false,
    }
}

/// True if an `if`/`while` condition `c` is — or, in a `let_chain` of `&&`-joined
/// conditions, directly contains — a `let_condition` (`let <pattern> = <expr>`)
/// whose pattern binds `name`. Only the top-level condition and direct `let_chain`
/// members are inspected: a `let` binding is in scope in the guarded body only at
/// those positions, so a `let_condition` nested inside a sub-expression (e.g. a
/// bool-valued inner `if let`) is correctly ignored — its binding does not reach
/// the body.
fn condition_let_binds_name(c: Node, name: &str, source: &[u8]) -> bool {
    let binds = |lc: Node| {
        lc.child_by_field_name("pattern")
            .is_some_and(|p| pattern_contains_identifier(p, name, source))
    };
    match c.kind() {
        "let_condition" => binds(c),
        "let_chain" => {
            let mut cursor = c.walk();
            c.named_children(&mut cursor)
                .filter(|n| n.kind() == "let_condition")
                .any(binds)
        }
        _ => false,
    }
}

/// True if `for_node`'s pattern is exactly the identifier `name`, its iterator is a
/// `range_expression` whose literal bounds give an interval that fits `target`
/// ([`interval_fits_int_target`]), and `name` is not re-bound between the loop
/// body's start and `cast_start`. Any other pattern shape (a tuple/ref pattern that
/// merely *contains* `name`) is not a range induction variable, so it is rejected —
/// and since such a binding shadows `name`, the caller must not look further out.
fn for_loop_range_fits_target(
    for_node: Node,
    name: &str,
    target: &str,
    cast_start: usize,
    source: &[u8],
) -> bool {
    let pattern_is_name = for_node
        .child_by_field_name("pattern")
        .is_some_and(|p| p.kind() == "identifier" && p.utf8_text(source) == Ok(name));
    if !pattern_is_name {
        return false;
    }
    let Some(iter) = for_node.child_by_field_name("value") else {
        return false;
    };
    if iter.kind() != "range_expression" {
        return false;
    }
    let Some((lo, hi)) = range_literal_interval(iter, source) else {
        return false;
    };
    if !interval_fits_int_target(lo, hi, target) {
        return false;
    }
    // A shadowing `let name` or a reassignment of `name` between the loop body's
    // opening brace and the cast breaks the link between the range and the value
    // the cast reads.
    let Some(body) = for_node.child_by_field_name("body") else {
        return false;
    };
    !name_rebound_in_range(body, body.start_byte(), cast_start, name, source)
}

/// The inclusive integer interval `[lo, hi]` (as `i128`) a `range_expression`'s
/// induction variable ranges over, or `None` when it is not a both-bounded literal
/// range. `a..b` (exclusive end) yields `[a, b - 1]`; `a..=b` (inclusive end)
/// yields `[a, b]`. Each bound must be a (possibly negated) integer literal
/// ([`signed_int_value`]) or a `const` resolving to one ([`resolve_const_int`]). A
/// half-open range (`a..` / `..b`), a deprecated `...` operator, an unresolved
/// bound, or an empty interval (`lo > hi`) yields `None`, leaving the cast flagged.
fn range_literal_interval(range: Node, source: &[u8]) -> Option<(i128, i128)> {
    // The operator is an anonymous token between the two operands; `..=` marks an
    // inclusive end, `..` an exclusive one. `...` is not supported.
    let mut cursor = range.walk();
    let inclusive = range.children(&mut cursor).find_map(|c| match c.kind() {
        "..=" => Some(true),
        ".." => Some(false),
        _ => None,
    })?;
    // Both bounds must be present (named-child expressions); a half-open range has
    // only one.
    if range.named_child_count() != 2 {
        return None;
    }
    let lo = resolve_range_bound(range.named_child(0)?, source)?;
    let hi_raw = resolve_range_bound(range.named_child(1)?, source)?;
    let hi = if inclusive { hi_raw } else { hi_raw.checked_sub(1)? };
    (lo <= hi).then_some((lo, hi))
}

/// Resolve a range bound to its `i128` value: a (possibly negated) integer literal
/// ([`signed_int_value`]), or a `const` identifier resolving to an integer literal
/// ([`resolve_const_int`]). Any other shape — a runtime variable, a call, an
/// expression — yields `None`, so the range stays unbounded and the cast flagged.
fn resolve_range_bound(node: Node, source: &[u8]) -> Option<i128> {
    if let Some(value) = signed_int_value(node, source) {
        return Some(value);
    }
    if node.kind() == "identifier" {
        return resolve_const_int(node, source).and_then(|v| i128::try_from(v).ok());
    }
    None
}

/// True if every value of the inclusive integer interval `[lo, hi]` is
/// representable in the integer type named by `target`. For an unsigned target the
/// interval must be non-negative (`lo >= 0`) and `hi` must not exceed the type's
/// maximum; for a signed target both ends must lie within `[T::MIN, T::MAX]`. A
/// non-integer target yields `false`.
fn interval_fits_int_target(lo: i128, hi: i128, target: &str) -> bool {
    if let Some(bits) = unsigned_int_bits(target) {
        if lo < 0 {
            return false;
        }
        let target_max: u128 = if bits >= 128 { u128::MAX } else { (1u128 << bits) - 1 };
        // `hi >= lo >= 0`, so the cast to `u128` is exact.
        (hi as u128) <= target_max
    } else if let Some(bits) = signed_int_bits(target) {
        let (target_min, target_max): (i128, i128) = if bits >= 128 {
            (i128::MIN, i128::MAX)
        } else {
            (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
        };
        lo >= target_min && hi <= target_max
    } else {
        false
    }
}

/// True if `cast` (a `type_cast_expression`) casts a signed integer to an
/// unsigned integer whose value an enclosing guard proves is non-negative, so
/// the cast cannot wrap — e.g. `Some(diff) if !diff.is_negative() => diff as u64`
/// or `if x >= 0 { x as u32 }`. For a signed→unsigned cast, non-negativity is
/// exactly the condition that makes it lossless: a non-negative `iN` fits any
/// `uM` with `M >= N` (the non-negative range of `iN` is `0..=2^(N-1)-1`, which
/// is within `0..=2^M-1`).
///
/// The exemption is deliberately narrow to stay sound — every condition must
/// hold:
///
/// - the operand is a bare `identifier` (`diff`), not an expression;
/// - the target is an **unsigned** integer (`u8`..`u128`/`usize`);
/// - the operand's source type, when it resolves from the AST, is a **signed**
///   integer `iN` no wider than the target (`target_bits >= N`); a narrowing
///   `i64 as u8` is not exempted because a non-negative `i64` can still exceed
///   `u8`. When the source type does not resolve (a match-arm binding has no
///   AST type annotation), the cast is exempted only into a **64-bit-or-wider**
///   unsigned target (`u64`/`u128`/`usize`) — the widening idiom the issue
///   describes; a non-negative-guarded cast into a narrow unresolved target
///   (`as u8`) stays flagged;
/// - a dominating guard proves the operand is non-negative — either a
///   `match_arm` guard on a pattern that binds the operand (`Some(diff) if
///   <guard>`), or an enclosing `if_expression` reached through its
///   `consequence` (`if <guard> { … diff as uM … }`). The recognized guard
///   forms are `!name.is_negative()`, `name.is_positive()`, `name >= 0`,
///   `name > 0`, `name > -1` (and the mirrored `0 <= name` / `0 < name` /
///   `-1 < name`);
/// - the operand is not re-bound (a shadowing `let name`) or reassigned inside
///   the guarded body before the cast, which would break the link between the
///   guard and the value the cast reads.
///
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`, which both
/// otherwise flag the cast because the operand's proven non-negativity is not
/// visible from the cast in isolation.
pub fn cast_operand_is_non_negative_guarded(cast: Node, source: &[u8]) -> bool {
    let Some(value) = cast.child_by_field_name("value") else {
        return false;
    };
    if value.kind() != "identifier" {
        return false;
    }
    let Ok(name) = value.utf8_text(source) else {
        return false;
    };
    // Target must be an unsigned integer; capture its bit width.
    let Some(target_bits) = cast
        .child_by_field_name("type")
        .and_then(|t| t.utf8_text(source).ok())
        .and_then(unsigned_int_bits)
    else {
        return false;
    };
    // Width gating: a non-negative value is lossless only if the unsigned target
    // is wide enough to hold the source's non-negative range. A resolved signed
    // source `iN` fits when `target_bits >= N`; an unresolved source (a match-arm
    // binding carries no AST type) is accepted only for the widening idiom into a
    // 64-bit-or-wider target.
    match find_identifier_type(cast, name, source).and_then(|t| signed_int_bits(&t)) {
        Some(source_bits) => {
            if target_bits < source_bits {
                return false;
            }
        }
        None => {
            if target_bits < 64 {
                return false;
            }
        }
    }
    enclosing_guard_proves_non_negative(cast, name, source)
}

/// True if `cast` (a `type_cast_expression`) is the body of a guard-less wildcard
/// `match` arm whose preceding sibling arms have collectively eliminated every
/// value outside the cast's unsigned target range — the saturating-clamp idiom:
///
/// ```ignore
/// match scrutinee {
///     val if val < 0 => 0,
///     val if val > 0xFF => 0xFF,
///     val => val as u8,   // reachable only when `0 <= val <= 0xFF`
/// }
/// ```
///
/// The wildcard arm is entered only when no preceding arm matched. A preceding
/// arm `val if <guard> => …` whose pattern is the bare binding `val`
/// (irrefutable) fails to match exactly when its guard is false, so the guard's
/// negation holds in the wildcard arm. Two such arms — one whose guard is
/// `val < 0` (negation `val >= 0`) and one whose guard is `val > N` where `N` is
/// the target type's maximum (negation `val <= N`) — together prove
/// `0 <= val <= N`, which fits the target exactly, so the cast cannot overflow.
///
/// The exemption is deliberately narrow to stay sound — every condition must
/// hold:
///
/// - the operand is a bare `identifier` (`val`), the value bound by the wildcard
///   arm;
/// - the target is an **unsigned** integer (`u8`..`u128`/`usize`); its maximum
///   `N` is computed from the target's bit width, not matched against a fixed
///   literal set;
/// - the cast is the wildcard arm's `value` body, that arm carries **no** guard,
///   and binds the operand as a bare irrefutable `identifier`;
/// - among the arms preceding the wildcard arm in the same `match`, one binds the
///   operand as a bare `identifier` with a guard proving `val < 0` (`val < 0`,
///   `val <= -1`, or the mirrored `0 > val` / `-1 >= val`), and one binds it with
///   a guard proving `val > N` (`val > N`, `val >= N + 1`, or the mirrored
///   `N < val` / `N + 1 <= val`), where `N` is the target's maximum.
///
/// A match missing either bound, an upper bound that is not the target's maximum,
/// or a guarded wildcard arm is not exempted: the cast may still overflow.
///
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`, which both
/// otherwise flag the cast because the multi-arm range proof is not visible from
/// the cast in isolation.
pub fn cast_operand_is_sibling_arm_bounded(cast: Node, source: &[u8]) -> bool {
    let Some(value) = cast.child_by_field_name("value") else {
        return false;
    };
    if value.kind() != "identifier" {
        return false;
    }
    let Ok(name) = value.utf8_text(source) else {
        return false;
    };
    // Target must be an unsigned integer; compute its maximum value.
    let Some(target_bits) = cast
        .child_by_field_name("type")
        .and_then(|t| t.utf8_text(source).ok())
        .and_then(unsigned_int_bits)
    else {
        return false;
    };
    let target_max: u128 = if target_bits >= 128 {
        u128::MAX
    } else {
        (1u128 << target_bits) - 1
    };

    // The cast must be the value body of a guard-less wildcard arm that binds the
    // operand as a bare irrefutable identifier.
    let Some((wild_arm, match_block)) = enclosing_match_arm(cast) else {
        return false;
    };
    let Some(wild_pattern) = wild_arm.child_by_field_name("pattern") else {
        return false;
    };
    if wild_pattern.child_by_field_name("condition").is_some() {
        return false;
    }
    if !pattern_is_bare_identifier(wild_pattern, name, source) {
        return false;
    }
    // A shadowing `let val` or a reassignment in the wildcard arm body before the
    // cast breaks the link between the sibling-arm bounds and the value the cast
    // reads, so the bounds no longer prove the cast operand fits.
    if let Some(body) = wild_arm.child_by_field_name("value")
        && name_rebound_before_cast(body, cast, name, source)
    {
        return false;
    }

    // Scan the arms preceding the wildcard arm for a floor guard (`val < 0`) and a
    // ceiling guard (`val > target_max`), each on a bare binding of `name`.
    let mut has_floor = false;
    let mut has_ceiling = false;
    let mut cursor = match_block.walk();
    for arm in match_block.children(&mut cursor) {
        if arm == wild_arm {
            break;
        }
        if arm.kind() != "match_arm" {
            continue;
        }
        let Some(pattern) = arm.child_by_field_name("pattern") else {
            continue;
        };
        if !pattern_is_bare_identifier(pattern, name, source) {
            continue;
        }
        let Some(condition) = pattern.child_by_field_name("condition") else {
            continue;
        };
        if guard_proves_below_zero(condition, name, source) {
            has_floor = true;
        }
        if guard_proves_above_max(condition, name, target_max, source) {
            has_ceiling = true;
        }
    }
    has_floor && has_ceiling
}

/// Walk up from `cast` to the nearest enclosing `match_arm` reached through its
/// `value` body, returning that arm and its parent `match_block`. Returns `None`
/// if the nearest such ancestor is reached through the arm's `pattern` (a cast in
/// a guard), if there is no enclosing arm before the `function_item` /
/// `closure_expression` boundary, or if the arm's parent is not a `match_block`.
fn enclosing_match_arm(cast: Node) -> Option<(Node, Node)> {
    let mut child = cast;
    while let Some(parent) = child.parent() {
        match parent.kind() {
            "function_item" | "closure_expression" => return None,
            "match_arm" => {
                if parent.child_by_field_name("value") != Some(child) {
                    return None;
                }
                let match_block = parent.parent()?;
                if match_block.kind() != "match_block" {
                    return None;
                }
                return Some((parent, match_block));
            }
            _ => {}
        }
        child = parent;
    }
    None
}

/// True if `match_pattern`'s pattern is the bare binding `identifier` `name` (an
/// irrefutable binding), ignoring any trailing guard. A refutable pattern
/// (`Some(name)`, `_`, a literal) returns false, so the only reason a guarded arm
/// with this pattern fails to match is its guard evaluating false.
fn pattern_is_bare_identifier(match_pattern: Node, name: &str, source: &[u8]) -> bool {
    match_pattern
        .named_child(0)
        .is_some_and(|inner| is_identifier_named(inner, name, source))
}

/// Split a comparison `condition` into `(left, operator, right)`, or `None` if it
/// is not a `binary_expression`.
fn comparison_parts(condition: Node) -> Option<(Node, Node, Node)> {
    if condition.kind() != "binary_expression" {
        return None;
    }
    Some((
        condition.child_by_field_name("left")?,
        condition.child_by_field_name("operator")?,
        condition.child_by_field_name("right")?,
    ))
}

/// True if `node` is the bare `identifier` `name`.
fn is_identifier_named(node: Node, name: &str, source: &[u8]) -> bool {
    node.kind() == "identifier" && node.utf8_text(source) == Ok(name)
}

/// True if `condition`'s failure proves `name >= 0` — i.e. the guard asserts the
/// value is negative: `name < 0`, `name <= -1`, or the mirrored `0 > name`,
/// `-1 >= name`.
fn guard_proves_below_zero(condition: Node, name: &str, source: &[u8]) -> bool {
    let Some((left, op, right)) = comparison_parts(condition) else {
        return false;
    };
    let Ok(op) = op.utf8_text(source) else {
        return false;
    };
    let ident_left = is_identifier_named(left, name, source);
    let ident_right = is_identifier_named(right, name, source);
    match op {
        "<" if ident_left => signed_int_value(right, source) == Some(0),
        "<=" if ident_left => signed_int_value(right, source) == Some(-1),
        ">" if ident_right => signed_int_value(left, source) == Some(0),
        ">=" if ident_right => signed_int_value(left, source) == Some(-1),
        _ => false,
    }
}

/// True if `condition`'s failure proves `name <= target_max` — i.e. the guard
/// asserts the value exceeds the target's maximum: `name > MAX`,
/// `name >= MAX + 1`, or the mirrored `MAX < name`, `MAX + 1 <= name`. For the
/// widest target (`u128`, `MAX == u128::MAX`) the `>= MAX + 1` forms are
/// unsatisfiable and correctly never match.
fn guard_proves_above_max(condition: Node, name: &str, target_max: u128, source: &[u8]) -> bool {
    let Some((left, op, right)) = comparison_parts(condition) else {
        return false;
    };
    let Ok(op) = op.utf8_text(source) else {
        return false;
    };
    let ident_left = is_identifier_named(left, name, source);
    let ident_right = is_identifier_named(right, name, source);
    let max_plus_one = target_max.checked_add(1);
    match op {
        ">" if ident_left => parse_int_literal(right, source) == Some(target_max),
        ">=" if ident_left => max_plus_one.is_some_and(|m| parse_int_literal(right, source) == Some(m)),
        "<" if ident_right => parse_int_literal(left, source) == Some(target_max),
        "<=" if ident_right => max_plus_one.is_some_and(|m| parse_int_literal(left, source) == Some(m)),
        _ => false,
    }
}

/// The bit width of a signed-integer type name (`i8` → 8, … `isize` → host
/// width), or `None` for any unsigned, float, or non-numeric type.
fn signed_int_bits(type_text: &str) -> Option<u16> {
    match type_text.trim() {
        "i8" => Some(8),
        "i16" => Some(16),
        "i32" => Some(32),
        "i64" => Some(64),
        "i128" => Some(128),
        "isize" => Some(usize::BITS as u16),
        _ => None,
    }
}

/// True if a dominating guard proves `name` is non-negative at `cast`. Two guard
/// sites count: a `match_arm` whose `pattern` binds `name` and whose guard
/// condition proves non-negativity (reached through the arm's `value` body), and
/// an enclosing `if_expression` reached through its `consequence`. The walk stops
/// at the enclosing `function_item` / `closure_expression` boundary. A guard
/// reached through an `else` branch (the `alternative`) does not apply, since it
/// is the negation of the condition.
fn enclosing_guard_proves_non_negative(cast: Node, name: &str, source: &[u8]) -> bool {
    let mut child = cast;
    while let Some(parent) = child.parent() {
        match parent.kind() {
            "function_item" | "closure_expression" => return false,
            "match_arm" => {
                if parent.child_by_field_name("value") == Some(child)
                    && let Some(pattern) = parent.child_by_field_name("pattern")
                    && pattern_contains_identifier(pattern, name, source)
                    && let Some(condition) = pattern.child_by_field_name("condition")
                    && condition_proves_non_negative(condition, name, source)
                    && !name_rebound_before_cast(child, cast, name, source)
                {
                    return true;
                }
            }
            "if_expression" => {
                if parent.child_by_field_name("consequence") == Some(child)
                    && let Some(condition) = parent.child_by_field_name("condition")
                    && condition_proves_non_negative(condition, name, source)
                    && !name_rebound_before_cast(child, cast, name, source)
                {
                    return true;
                }
            }
            _ => {}
        }
        child = parent;
    }
    false
}

/// True if `condition` proves the identifier `name` is non-negative (`>= 0`).
///
/// Recognized forms:
/// - `!name.is_negative()` — a non-negative signed integer;
/// - `name.is_positive()` — a strictly positive signed integer (also `>= 0`);
/// - `name >= 0` / `name > 0` / `name > -1` (identifier on the left);
/// - `0 <= name` / `0 < name` / `-1 < name` (identifier on the right).
fn condition_proves_non_negative(condition: Node, name: &str, source: &[u8]) -> bool {
    match condition.kind() {
        // `!name.is_negative()`
        "unary_expression" => {
            condition
                .child(0)
                .and_then(|op| op.utf8_text(source).ok())
                .is_some_and(|op| op == "!")
                && condition
                    .named_child(0)
                    .is_some_and(|inner| ident_method_call(inner, name, "is_negative", source))
        }
        // `name.is_positive()`
        "call_expression" => ident_method_call(condition, name, "is_positive", source),
        // `name >= 0` / `name > 0` / `name > -1` and the mirrored forms.
        "binary_expression" => binary_proves_non_negative(condition, name, source),
        _ => false,
    }
}

/// True if `node` is a no-argument method call `<name>.<method>()` on the bare
/// identifier `name` — e.g. `diff.is_negative()`.
fn ident_method_call(node: Node, name: &str, method: &str, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    if node
        .child_by_field_name("arguments")
        .is_some_and(|args| args.named_child_count() > 0)
    {
        return false;
    }
    let Some(function) = node.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    function
        .child_by_field_name("value")
        .is_some_and(|recv| recv.kind() == "identifier" && recv.utf8_text(source) == Ok(name))
        && function
            .child_by_field_name("field")
            .and_then(|f| f.utf8_text(source).ok())
            == Some(method)
}

/// True if `condition` is a comparison proving `name >= 0`: `name >= 0`,
/// `name > 0`, `name > -1` (identifier on the left), or the mirrored
/// `0 <= name`, `0 < name`, `-1 < name` (identifier on the right). A bound that
/// does not parse, or that fails to establish non-negativity, returns false.
fn binary_proves_non_negative(condition: Node, name: &str, source: &[u8]) -> bool {
    let (Some(left), Some(op), Some(right)) = (
        condition.child_by_field_name("left"),
        condition
            .child_by_field_name("operator")
            .and_then(|o| o.utf8_text(source).ok()),
        condition.child_by_field_name("right"),
    ) else {
        return false;
    };
    let ident_left = left.kind() == "identifier" && left.utf8_text(source) == Ok(name);
    let ident_right = right.kind() == "identifier" && right.utf8_text(source) == Ok(name);
    match op {
        // `name >= 0` — `0` proves non-negativity exactly.
        ">=" if ident_left => signed_int_value(right, source) == Some(0),
        // `name > 0` and `name > -1` — both prove `name >= 0`.
        ">" if ident_left => matches!(signed_int_value(right, source), Some(0) | Some(-1)),
        // `0 <= name`
        "<=" if ident_right => signed_int_value(left, source) == Some(0),
        // `0 < name` and `-1 < name`
        "<" if ident_right => matches!(signed_int_value(left, source), Some(0) | Some(-1)),
        _ => false,
    }
}

/// Parse an integer literal node's value as `i128`, handling an optional leading
/// `-` (`unary_expression`) and a type suffix / digit separators. Returns `None`
/// for non-literal nodes or values that do not parse.
fn signed_int_value(node: Node, source: &[u8]) -> Option<i128> {
    match node.kind() {
        "integer_literal" => parse_int_literal(node, source).and_then(|v| i128::try_from(v).ok()),
        "unary_expression" => {
            let is_neg = node
                .child(0)
                .and_then(|op| op.utf8_text(source).ok())
                .is_some_and(|op| op == "-");
            if !is_neg {
                return None;
            }
            let inner = node.named_child(0)?;
            if inner.kind() != "integer_literal" {
                return None;
            }
            parse_int_literal(inner, source)
                .and_then(|v| i128::try_from(v).ok())
                .map(|v| -v)
        }
        _ => None,
    }
}

/// The bit width of an unsigned-integer type name (`u8` → 8, … `usize` → host
/// width), or `None` for any signed, float, or non-numeric type.
fn unsigned_int_bits(type_text: &str) -> Option<u16> {
    match type_text.trim() {
        "u8" => Some(8),
        "u16" => Some(16),
        "u32" => Some(32),
        "u64" => Some(64),
        "u128" => Some(128),
        "usize" => Some(usize::BITS as u16),
        _ => None,
    }
}

/// True if some enclosing `if_expression` bounds `name` against an integer
/// literal that proves a value fits an unsigned target of `target_bits` width,
/// whether the cast is reached through the `consequence` (the condition directly
/// upper-bounds the value) or the `alternative` (the `else` / `else if` branch,
/// where the condition's negation upper-bounds it).
///
/// Ascends via `parent()`. At each enclosing `if_expression`, the child we came
/// from is either its `consequence` — then the condition itself must upper-bound
/// `name` (`condition_upper_bounds`) — or its `alternative` — then the
/// condition's negation must upper-bound `name` (`negated_condition_upper_bounds`),
/// since the `else` branch is entered exactly when the condition is false. Either
/// way the bound holds only while `name` is unchanged between the branch entry and
/// the cast. The walk stops at the enclosing `function_item` / `closure_expression`
/// boundary.
fn enclosing_upper_bound_fits(cast: Node, name: &str, target_bits: u16, source: &[u8]) -> bool {
    let mut child = cast;
    while let Some(parent) = child.parent() {
        match parent.kind() {
            "function_item" | "closure_expression" => return false,
            "if_expression" => {
                let condition = parent.child_by_field_name("condition");
                // Through the `consequence` the condition directly upper-bounds
                // the value; through the `alternative` (the `else` branch, reached
                // only when the condition is false) its negation does.
                let bounded = if parent.child_by_field_name("consequence") == Some(child) {
                    condition.is_some_and(|c| condition_upper_bounds(c, name, target_bits, source))
                } else if parent.child_by_field_name("alternative") == Some(child) {
                    condition
                        .is_some_and(|c| negated_condition_upper_bounds(c, name, target_bits, source))
                } else {
                    false
                };
                // The guard proves the value of `name` only as long as it is
                // unchanged between the branch entry and the cast. A re-binding (a
                // shadowing `let name`) or a reassignment (`name = …` / `name += …`)
                // inside the branch before the cast invalidates the bound.
                if bounded && !name_rebound_before_cast(child, cast, name, source) {
                    return true;
                }
            }
            _ => {}
        }
        child = parent;
    }
    false
}

/// True if a preceding early-exit guard in the cast's enclosing `block` proves the
/// operand `name` fits the unsigned target of `target_bits` width: a statement
/// `if name > N { <diverges> }` (no `else`) whose `then` branch unconditionally
/// diverges, so control reaches the cast only when the guard's condition is false
/// (`name <= N`).
///
/// The guard's condition must lower-bound `name` against an integer literal whose
/// negation fits the target (`negated_condition_upper_bounds`), the `then` branch
/// must unconditionally diverge (`block_diverges`), and `name` must not be re-bound
/// between the guard and the cast — otherwise the bound no longer holds at the cast
/// site. Guards are scanned nearest-last: a closer guard whose window is clean
/// still applies even if an earlier one was invalidated by a re-binding.
fn preceding_exit_guard_upper_bounds(
    cast: Node,
    name: &str,
    target_bits: u16,
    source: &[u8],
) -> bool {
    let Some((block, cast_stmt)) = enclosing_block_statement(cast) else {
        return false;
    };
    let cast_start = cast.start_byte();
    let mut cursor = block.walk();
    for stmt in block.named_children(&mut cursor) {
        if stmt.id() == cast_stmt.id() {
            break;
        }
        let Some(if_expr) = exit_guard_if(stmt) else {
            continue;
        };
        let bounds = if_expr
            .child_by_field_name("condition")
            .is_some_and(|c| negated_condition_upper_bounds(c, name, target_bits, source));
        if !bounds {
            continue;
        }
        let diverges = if_expr
            .child_by_field_name("consequence")
            .is_some_and(|c| block_diverges(c, source));
        if !diverges {
            continue;
        }
        if !name_rebound_in_range(block, if_expr.end_byte(), cast_start, name, source) {
            return true;
        }
    }
    false
}

/// If `stmt` is (or wraps) an `if_expression` with no `else` branch — a pure
/// early-exit guard — return that `if_expression`. An `if`/`else` is rejected: a
/// value reaching a statement after the `if` would then also flow through the
/// `else`, whose effects this analysis does not track.
fn exit_guard_if(stmt: Node) -> Option<Node> {
    let if_expr = match stmt.kind() {
        "if_expression" => stmt,
        "expression_statement" => stmt.named_child(0).filter(|c| c.kind() == "if_expression")?,
        _ => return None,
    };
    if if_expr.child_by_field_name("alternative").is_some() {
        return None;
    }
    Some(if_expr)
}

/// True if `block` (a `block` node) unconditionally diverges — its final element
/// is a `return` / `break` / `continue` expression or a diverging macro
/// (`panic!` / `unreachable!` / `todo!` / `unimplemented!`), so control never falls
/// through to the statement following the block.
fn block_diverges(block: Node, source: &[u8]) -> bool {
    if block.kind() != "block" {
        return false;
    }
    let mut cursor = block.walk();
    block
        .named_children(&mut cursor)
        .last()
        .is_some_and(|last| node_diverges(last, source))
}

/// True if `node` is a diverging tail — a `return` / `break` / `continue`
/// expression, a diverging macro invocation, or an `expression_statement` wrapping
/// one of those.
fn node_diverges(node: Node, source: &[u8]) -> bool {
    match node.kind() {
        "return_expression" | "break_expression" | "continue_expression" => true,
        "expression_statement" => node.named_child(0).is_some_and(|c| node_diverges(c, source)),
        "macro_invocation" => node
            .child_by_field_name("macro")
            .and_then(|m| m.utf8_text(source).ok())
            .is_some_and(|n| matches!(n, "panic" | "unreachable" | "todo" | "unimplemented")),
        _ => false,
    }
}

/// True if `name` is re-bound or reassigned anywhere in `consequence` *before*
/// `cast` (by source position) — a shadowing `let name`, an `assignment_expression`
/// to `name`, or a `compound_assignment_expr` to `name`. Such a write breaks the
/// link between the guard's bound and the value the cast reads.
fn name_rebound_before_cast(consequence: Node, cast: Node, name: &str, source: &[u8]) -> bool {
    let cast_start = cast.start_byte();
    let mut cursor = consequence.walk();
    let mut stack = vec![consequence];
    while let Some(node) = stack.pop() {
        if node.start_byte() >= cast_start {
            continue;
        }
        let rebinds = match node.kind() {
            "let_declaration" => node
                .child_by_field_name("pattern")
                .is_some_and(|p| pattern_contains_identifier(p, name, source)),
            "assignment_expression" | "compound_assignment_expr" => node
                .child_by_field_name("left")
                .is_some_and(|l| l.kind() == "identifier" && l.utf8_text(source) == Ok(name)),
            _ => false,
        };
        if rebinds {
            return true;
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if `condition` is a comparison that upper-bounds the identifier `name`
/// against an integer-literal bound — or a `const` identifier resolving to one
/// ([`resolve_int_bound`]) — proving a value fits an unsigned target of
/// `target_bits` width.
///
/// Recognized shapes (the bound `N` is the *upper* bound on `name`):
/// - `name < N` / `name <= N` (identifier on the left);
/// - `N > name` / `N >= name` (identifier on the right).
///
/// `name < N` proves `name <= N - 1`, which fits when `N <= 2^bits`; `name <= N`
/// proves `name <= N`, which fits when `N <= 2^bits - 1`. A bound that does not
/// resolve to an integer literal, or a bound too large for the target, returns
/// false.
fn condition_upper_bounds(condition: Node, name: &str, target_bits: u16, source: &[u8]) -> bool {
    if condition.kind() != "binary_expression" {
        return false;
    }
    let (Some(left), Some(op), Some(right)) = (
        condition.child_by_field_name("left"),
        condition
            .child_by_field_name("operator")
            .and_then(|o| o.utf8_text(source).ok()),
        condition.child_by_field_name("right"),
    ) else {
        return false;
    };

    // Normalize to `<identifier> <op'> <literal>`, mapping `>`/`>=` (identifier
    // on the right) onto the corresponding `<`/`<=`.
    let (bound_node, inclusive) = match op {
        "<" => (ident_then_bound(left, right, name, source), false),
        "<=" => (ident_then_bound(left, right, name, source), true),
        ">" => (ident_then_bound(right, left, name, source), false),
        ">=" => (ident_then_bound(right, left, name, source), true),
        _ => return false,
    };
    let Some(bound) = bound_node.and_then(|n| resolve_int_bound(n, source)) else {
        return false;
    };
    // Max value the target can hold (2^bits - 1), computed without overflowing
    // the `1u128 << 128` shift for a u128 target.
    let target_max: u128 = if target_bits >= 128 {
        u128::MAX
    } else {
        (1u128 << target_bits) - 1
    };
    if inclusive {
        // `name <= N` proves `name <= N`; safe when `N <= target_max`.
        bound <= target_max
    } else {
        // `name < N` proves `name <= N - 1`; safe when `N - 1 <= target_max`.
        // `checked_sub` rejects `name < 0` (an unsatisfiable guard) without
        // adding `1` to `target_max`, which would overflow for a u128 target.
        bound.checked_sub(1).is_some_and(|max_value| max_value <= target_max)
    }
}

/// True if the NEGATION of `condition` upper-bounds the identifier `name` against
/// an integer-literal bound — or a `const` identifier resolving to one
/// ([`resolve_int_bound`]) — proving a value fits an unsigned target of
/// `target_bits` width — the bound that holds in the `else` / `alternative`
/// branch of `if <condition> { … }`, where the condition is known to be false.
///
/// Recognized shapes (the literal *lower*-bounds `name` in the condition, so its
/// negation is an *upper* bound in the else branch):
/// - `name > N` (negates to `name <= N`) / `name >= N` (negates to `name < N`);
/// - the mirrored `N < name` / `N <= name`.
///
/// `name <= N` fits when `N <= 2^bits - 1`; `name < N` proves `name <= N - 1`,
/// which fits when `N - 1 <= 2^bits - 1`. The bound `N` may be an integer literal
/// or a `const` identifier resolving to one ([`resolve_int_bound`]); a bound that
/// does not resolve, or one too large for the target, returns false.
fn negated_condition_upper_bounds(
    condition: Node,
    name: &str,
    target_bits: u16,
    source: &[u8],
) -> bool {
    if condition.kind() != "binary_expression" {
        return false;
    }
    let (Some(left), Some(op), Some(right)) = (
        condition.child_by_field_name("left"),
        condition
            .child_by_field_name("operator")
            .and_then(|o| o.utf8_text(source).ok()),
        condition.child_by_field_name("right"),
    ) else {
        return false;
    };

    // The condition lower-bounds `name`; its negation upper-bounds it. `>` negates
    // to an inclusive `<= N` bound, `>=` to an exclusive `< N` bound. `>` / `>=`
    // carry the identifier on the left; the mirrored `<` / `<=` carry it on the
    // right (`N < name` ≡ `name > N`).
    let (bound_node, inclusive) = match op {
        ">" => (ident_then_bound(left, right, name, source), true),
        ">=" => (ident_then_bound(left, right, name, source), false),
        "<" => (ident_then_bound(right, left, name, source), true),
        "<=" => (ident_then_bound(right, left, name, source), false),
        _ => return false,
    };
    let Some(bound) = bound_node.and_then(|n| resolve_int_bound(n, source)) else {
        return false;
    };
    // Max value the target can hold (2^bits - 1), computed without overflowing the
    // `1u128 << 128` shift for a u128 target.
    let target_max: u128 = if target_bits >= 128 {
        u128::MAX
    } else {
        (1u128 << target_bits) - 1
    };
    if inclusive {
        // `name <= N` proves `name <= N`; safe when `N <= target_max`.
        bound <= target_max
    } else {
        // `name < N` proves `name <= N - 1`; safe when `N - 1 <= target_max`.
        // `checked_sub` rejects an empty (`name < 0`) bound without adding `1` to
        // `target_max`, which would overflow for a u128 target.
        bound.checked_sub(1).is_some_and(|max_value| max_value <= target_max)
    }
}

/// If `ident` is the identifier `name` and `bound` is an integer literal or a
/// plain `identifier` (a candidate `const`-identifier bound, resolved later by
/// [`resolve_int_bound`]), return the `bound` node; otherwise `None`.
fn ident_then_bound<'a>(
    ident: Node<'a>,
    bound: Node<'a>,
    name: &str,
    source: &[u8],
) -> Option<Node<'a>> {
    if ident.kind() == "identifier"
        && ident.utf8_text(source).is_ok_and(|t| t == name)
        && matches!(bound.kind(), "integer_literal" | "identifier")
    {
        Some(bound)
    } else {
        None
    }
}

/// Resolve `node` — an upper-bound operand — to its `u128` value. An
/// `integer_literal` is parsed directly; an `identifier` is resolved to an
/// enclosing `const` whose value is itself an integer literal (see
/// [`resolve_const_int`]). Any other shape, or a `const` whose value is not a
/// plain integer literal, yields `None`, leaving the cast unexempted.
fn resolve_int_bound(node: Node, source: &[u8]) -> Option<u128> {
    match node.kind() {
        "integer_literal" => parse_int_literal(node, source),
        "identifier" => resolve_const_int(node, source),
        _ => None,
    }
}

/// Resolve a `const`-identifier `ident` to its integer-literal value by purely
/// structural tree-sitter lookup (no type inference): walk the ancestors of
/// `ident` and, at each enclosing scope, scan its direct children for a
/// `const_item` whose `name` matches `ident` and whose `value` is an
/// `integer_literal`, returning that literal's parsed value. The nearest such
/// `const` wins. Returns `None` when no matching `const` is found, or the
/// matched `const`'s value is not a plain integer literal (e.g. `1 << 53`, a
/// call, or another const) — keeping the bound, and thus the cast, unexempted.
///
/// A nearer non-`const` binding of the same name (a `let` in an enclosing block,
/// or a parameter of the enclosing fn/closure) shadows any outer `const`: the
/// bound then reads a runtime local of unknown value, so the walk stops and
/// returns `None` rather than resolving the shadowed `const`.
fn resolve_const_int(ident: Node, source: &[u8]) -> Option<u128> {
    let target = ident.utf8_text(source).ok()?;
    let mut scope = ident.parent();
    while let Some(node) = scope {
        if scope_binds_name_nonconst(node, target, source) {
            return None;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "const_item"
                && child.child_by_field_name("name").and_then(|n| n.utf8_text(source).ok())
                    == Some(target)
            {
                let value = child.child_by_field_name("value")?;
                return (value.kind() == "integer_literal")
                    .then(|| parse_int_literal(value, source))
                    .flatten();
            }
        }
        scope = node.parent();
    }
    None
}

/// True if `scope` introduces a non-`const` binding of `name`: a `let_declaration`
/// whose pattern binds `name` (when `scope` is a `block` or similar body), or a
/// `parameter` binding `name` (when `scope` is a `function_item` /
/// `closure_expression`). Such a binding shadows any outer `const` of the same
/// name, so [`resolve_const_int`] must not resolve past it.
fn scope_binds_name_nonconst(scope: Node, name: &str, source: &[u8]) -> bool {
    let binds_pattern = |node: Node| {
        node.child_by_field_name("pattern")
            .is_some_and(|pattern| pattern_contains_identifier(pattern, name, source))
    };
    match scope.kind() {
        "function_item" | "closure_expression" => scope
            .child_by_field_name("parameters")
            .is_some_and(|params| {
                let mut cursor = params.walk();
                params
                    .named_children(&mut cursor)
                    .filter(|p| p.kind() == "parameter")
                    .any(binds_pattern)
            }),
        _ => {
            let mut cursor = scope.walk();
            scope
                .named_children(&mut cursor)
                .filter(|c| c.kind() == "let_declaration")
                .any(binds_pattern)
        }
    }
}

/// Parse an `integer_literal` node's value as `u128`, accepting decimal, hex
/// (`0xFF`), octal (`0o77`), and binary (`0b1010`) forms, stripping a type suffix
/// (`256u32`) and digit separators (`65_536`). Returns `None` for an empty digit
/// run or an out-of-range value.
fn parse_int_literal(node: Node, source: &[u8]) -> Option<u128> {
    let text = node.utf8_text(source).ok()?;
    let (radix, body) = match text.as_bytes() {
        [b'0', b'x' | b'X', ..] => (16, &text[2..]),
        [b'0', b'o' | b'O', ..] => (8, &text[2..]),
        [b'0', b'b' | b'B', ..] => (2, &text[2..]),
        _ => (10, text),
    };
    let digits: String = body
        .chars()
        .take_while(|c| c.is_digit(radix) || *c == '_')
        .filter(|c| *c != '_')
        .collect();
    if digits.is_empty() {
        return None;
    }
    u128::from_str_radix(&digits, radix).ok()
}

/// True if `cast` (a `type_cast_expression`) narrows an identifier whose value a
/// preceding `assert!` / `debug_assert!` in the same block proves fits the
/// unsigned target type — the runtime-checked counterpart of the compile-time
/// branch guard handled by [`cast_operand_is_range_guarded`]:
///
/// ```ignore
/// assert!(x <= u8::MAX as u64);
/// let y = x as u8;        // the assert proves `x` fits u8
/// ```
///
/// The exemption is deliberately narrow to stay sound — every condition must
/// hold:
///
/// - the operand is a bare `identifier` (`x`), not an expression;
/// - the target is an **unsigned** integer (`u8`..`u128`/`usize`);
/// - the operand's source type resolves from the AST to an **unsigned** integer,
///   which proves the value is non-negative (lower bound 0). An unresolved or
///   signed source is not exempted: an upper-bound assertion alone cannot rule
///   out a negative value wrapping on the cast;
/// - a statement **before** the cast in the same enclosing `block` is an
///   `assert!` / `debug_assert!` whose first argument upper-bounds the SAME
///   identifier so the value provably fits the target: `x < BOUND` /
///   `x <= BOUND` (and the symmetric `BOUND > x` / `BOUND >= x`), where `BOUND`
///   is either an integer literal `N` in range (`< N` needs `N <= 2^bits`,
///   `<= N` needs `N <= 2^bits - 1`) or an unsigned type's `::MAX` whose own
///   maximum does not exceed the target's (`u8::MAX`, optionally widened by a
///   trailing `as <type>`);
/// - the identifier is not re-bound (a shadowing `let x`) or reassigned
///   (`x = …` / `x += …`) between the assertion and the cast, which would break
///   the link between the asserted bound and the value the cast reads.
///
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`, which both
/// otherwise flag the narrowing because the asserted bound is not visible from
/// the cast in isolation.
pub fn cast_operand_is_assert_bounded(cast: Node, source: &[u8]) -> bool {
    let Some(value) = cast.child_by_field_name("value") else {
        return false;
    };
    if value.kind() != "identifier" {
        return false;
    }
    let Ok(name) = value.utf8_text(source) else {
        return false;
    };
    let Some(target_bits) = cast
        .child_by_field_name("type")
        .and_then(|t| t.utf8_text(source).ok())
        .and_then(unsigned_int_bits)
    else {
        return false;
    };
    // The source must resolve to an unsigned integer so the value is provably
    // non-negative; an upper-bound assertion cannot otherwise rule out underflow.
    if find_identifier_type(cast, name, source)
        .and_then(|t| unsigned_int_bits(&t))
        .is_none()
    {
        return false;
    }
    // The cast's own statement inside the enclosing `block`; preceding siblings
    // are the statements whose asserts can bound the value.
    let Some((block, cast_stmt)) = enclosing_block_statement(cast) else {
        return false;
    };
    let cast_start = cast.start_byte();
    let mut cursor = block.walk();
    for stmt in block.named_children(&mut cursor) {
        if stmt.id() == cast_stmt.id() {
            break;
        }
        let Some(macro_node) = assert_macro_invocation(stmt, source) else {
            continue;
        };
        if !assert_upper_bounds(macro_node, name, target_bits, source) {
            continue;
        }
        // The bound proves the value of `name` only while it is unchanged
        // between the assertion and the cast. A re-binding (shadowing `let name`)
        // or reassignment (`name = …` / `name += …`) in any intervening
        // statement invalidates the bound.
        if name_rebound_in_range(block, macro_node.end_byte(), cast_start, name, source) {
            return false;
        }
        return true;
    }
    false
}

/// The enclosing `block` and the cast's own statement node within it (the direct
/// child of the block that contains `cast`), or `None` if the cast is not inside
/// a block statement (e.g. it is a tail expression nested in another expression).
fn enclosing_block_statement(cast: Node) -> Option<(Node, Node)> {
    let mut child = cast;
    while let Some(parent) = child.parent() {
        if parent.kind() == "block" {
            return Some((parent, child));
        }
        child = parent;
    }
    None
}

/// If `stmt` is (or wraps) an `assert!` / `debug_assert!` invocation, return its
/// `macro_invocation` node. An assertion appears as an `expression_statement`
/// wrapping the `macro_invocation`.
fn assert_macro_invocation<'a>(stmt: Node<'a>, source: &[u8]) -> Option<Node<'a>> {
    let macro_node = match stmt.kind() {
        "macro_invocation" => stmt,
        "expression_statement" => stmt.named_child(0).filter(|c| c.kind() == "macro_invocation")?,
        _ => return None,
    };
    let name = macro_node
        .child_by_field_name("macro")
        .and_then(|m| m.utf8_text(source).ok())?;
    matches!(name, "assert" | "debug_assert").then_some(macro_node)
}

/// True if the first argument of the `assert!` / `debug_assert!` `macro_node`
/// upper-bounds the identifier `name` so the value provably fits an unsigned
/// target of `target_bits` width.
///
/// The macro's arguments are an unparsed `token_tree` (tree-sitter does not
/// build a `binary_expression` inside it), so the comparison is read from the
/// flat token sequence: the identifier `name`, a comparison operator, and the
/// bound. `name <op> BOUND` and the symmetric `BOUND <op> name` are both read.
fn assert_upper_bounds(macro_node: Node, name: &str, target_bits: u16, source: &[u8]) -> bool {
    let Some(tokens) = macro_node
        .named_children(&mut macro_node.walk())
        .find(|c| c.kind() == "token_tree")
    else {
        return false;
    };
    // The `token_tree`'s own outer `(` … `)` delimiters are its first and last
    // children; strip them, then locate the comparison operator (only the first
    // argument matters; a `,` ends it). Tokens before it are the left side,
    // tokens after are the right.
    let mut cursor = tokens.walk();
    let children: Vec<Node> = tokens.children(&mut cursor).collect();
    let children = strip_paren_tokens(&children);
    // Only the first argument bounds the value; a `,` ends it (an `assert!`
    // message argument follows). Truncate to it before reading the comparison.
    let first_arg = match children.iter().position(|t| t.utf8_text(source) == Ok(",")) {
        Some(i) => &children[..i],
        None => children,
    };
    let mut op_idx = None;
    for (i, tok) in first_arg.iter().enumerate() {
        if matches!(tok.utf8_text(source), Ok("<" | "<=" | ">" | ">=")) {
            op_idx = Some(i);
            break;
        }
    }
    let Some(op_idx) = op_idx else { return false };
    let op = first_arg[op_idx].utf8_text(source).unwrap_or("");
    let left = &first_arg[..op_idx];
    let right = &first_arg[op_idx + 1..];

    // Normalize to `<name> <upper-bound-op> <bound>`: `<`/`<=` keep the bound on
    // the right, `>`/`>=` put `name` on the right and the bound on the left.
    let (bound_tokens, inclusive) = match op {
        "<" if tokens_are_identifier(left, name, source) => (right, false),
        "<=" if tokens_are_identifier(left, name, source) => (right, true),
        ">" if tokens_are_identifier(right, name, source) => (left, false),
        ">=" if tokens_are_identifier(right, name, source) => (left, true),
        _ => return false,
    };
    let Some(bound) = bound_upper_value(bound_tokens, source) else {
        return false;
    };
    let target_max: u128 = if target_bits >= 128 {
        u128::MAX
    } else {
        (1u128 << target_bits) - 1
    };
    if inclusive {
        bound <= target_max
    } else {
        // `name < N` proves `name <= N - 1`. `checked_sub` rejects `name < 0`
        // without adding 1 to `target_max` (which would overflow a u128 target).
        bound.checked_sub(1).is_some_and(|max_value| max_value <= target_max)
    }
}

/// True if `tokens` (the side of a token-tree comparison, parens stripped) is
/// exactly the single identifier `name`.
fn tokens_are_identifier(tokens: &[Node], name: &str, source: &[u8]) -> bool {
    let inner = strip_paren_tokens(tokens);
    matches!(inner, [tok] if tok.kind() == "identifier" && tok.utf8_text(source) == Ok(name))
}

/// The upper-bound value a comparison's bound side proves, read from raw tokens:
/// either an integer literal `N`, or an unsigned type's `::MAX` (e.g. `u8::MAX`),
/// each optionally widened by a trailing `as <type>` cast (ignored — it does not
/// change the value). Returns `None` for any other shape (a method call, field,
/// arithmetic, or a non-`MAX` associated const).
fn bound_upper_value(tokens: &[Node], source: &[u8]) -> Option<u128> {
    let tokens = strip_paren_tokens(tokens);
    // Drop a trailing `as <type>` widening — `u8::MAX as u64` has value u8::MAX.
    let core = match tokens.iter().position(|t| t.utf8_text(source) == Ok("as")) {
        Some(i) => &tokens[..i],
        None => tokens,
    };
    match core {
        // A bare integer literal: `256`, `65_536`.
        [lit] if lit.kind() == "integer_literal" => parse_int_literal(*lit, source),
        // `<utype>::MAX` — value is that type's maximum.
        [ty, sep, max]
            if sep.utf8_text(source) == Ok("::")
                && max.utf8_text(source) == Ok("MAX") =>
        {
            ty.utf8_text(source)
                .ok()
                .and_then(unsigned_int_bits)
                .map(|bits| {
                    if bits >= 128 {
                        u128::MAX
                    } else {
                        (1u128 << bits) - 1
                    }
                })
        }
        _ => None,
    }
}

/// Strip a single layer of wrapping `(` … `)` tokens from a token-tree slice.
fn strip_paren_tokens<'a>(tokens: &'a [Node<'a>]) -> &'a [Node<'a>] {
    match tokens {
        [first, mid @ .., last]
            if first.kind() == "(" && last.kind() == ")" =>
        {
            mid
        }
        _ => tokens,
    }
}

/// True if `name` is re-bound or reassigned anywhere in `block` whose write
/// position lies in `[start, end)` — a shadowing `let name`, an
/// `assignment_expression`, or a `compound_assignment_expr` to `name`. Used to
/// invalidate an asserted bound when the value is overwritten before the cast.
fn name_rebound_in_range(
    block: Node,
    start: usize,
    end: usize,
    name: &str,
    source: &[u8],
) -> bool {
    let mut cursor = block.walk();
    let mut stack: Vec<Node> = block.children(&mut cursor).collect();
    while let Some(node) = stack.pop() {
        if node.start_byte() < start || node.start_byte() >= end {
            // Still descend into nodes that merely span the range boundary.
            if node.end_byte() > start && node.start_byte() < end {
                let mut c = node.walk();
                stack.extend(node.children(&mut c));
            }
            continue;
        }
        let rebinds = match node.kind() {
            "let_declaration" => node
                .child_by_field_name("pattern")
                .is_some_and(|p| pattern_contains_identifier(p, name, source)),
            "assignment_expression" | "compound_assignment_expr" => node
                .child_by_field_name("left")
                .is_some_and(|l| l.kind() == "identifier" && l.utf8_text(source) == Ok(name)),
            _ => false,
        };
        if rebinds {
            return true;
        }
        let mut c = node.walk();
        stack.extend(node.children(&mut c));
    }
    false
}

/// True if the operand of `cast` (a `type_cast_expression`) is, at its
/// outermost level, a bitwise operation — a shift (`>>`/`<<`) or a bit op
/// (`&`/`|`/`^`). Such a cast is deliberate bit manipulation, where narrowing
/// to the target width is the intent rather than an accidental truncation:
/// `(x >> 24) as u8` extracts a byte, `(x & 0xFF) as u8` masks one. A fallible
/// `try_into()` would be the wrong remediation for these shapes, so both
/// numeric-cast rules treat them as lossless. Parentheses around the operand
/// are transparent.
pub fn cast_operand_is_bitwise(cast: Node, source: &[u8]) -> bool {
    let Some(mut value) = cast.child_by_field_name("value") else {
        return false;
    };
    while value.kind() == "parenthesized_expression" {
        let Some(inner) = value.named_child(0) else {
            return false;
        };
        value = inner;
    }
    if value.kind() != "binary_expression" {
        return false;
    }
    value
        .child_by_field_name("operator")
        .and_then(|op| op.utf8_text(source).ok())
        .is_some_and(|op| matches!(op, ">>" | "<<" | "&" | "|" | "^"))
}

/// True when `cast` (a `type_cast_expression`) narrows `(x % N) as uT` where the
/// modulo's right operand `N` is an integer literal small enough that the
/// remainder always fits the unsigned target, and the dividend `x` is provably
/// non-negative from the AST.
///
/// For a non-negative `x`, Rust's `%` yields a value in `[0, N - 1]` (the
/// remainder follows the sign of the dividend, so a non-negative dividend gives a
/// non-negative remainder). When `N - 1 <= uT::MAX` that whole range is
/// representable, so the cast is lossless — `(width % 256) as u8`,
/// `(nanos % 1_000_000) as u32`.
///
/// Soundness hinges on the dividend being non-negative: a SIGNED `x % N` can be
/// negative (`-1i32 % 256 == -1`), and `(-1i32) as u8` wraps to `255` — a
/// genuinely lossy cast the rules must keep flagging. Unlike the bitwise-AND mask
/// of [`cast_operand_is_bitwise`] (non-negative regardless of sign), modulo needs
/// an explicit unsigned proof. The exemption therefore requires:
///
/// - the target is an **unsigned** integer (`u8`..`u128`/`usize`); a signed target
///   is never exempt;
/// - the right operand is an integer literal `N` with `N - 1 <= uT::MAX` (i.e.
///   `N <= 2^bits`); `(x % 300) as u8` stays flagged because `299` exceeds `u8`;
/// - the dividend is **provably non-negative** from the AST per
///   [`expr_is_provably_nonneg`]. A dividend whose type cannot be resolved (an
///   unannotated local, or a method return like `.as_nanos()` whose type lives in
///   std) is left unproven, so it stays flagged — a conservative false-negative,
///   never an unsound false-positive.
///
/// Any `parenthesized_expression` layers around the operand are transparent.
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`.
pub fn cast_operand_is_modulo_bounded(cast: Node, source: &[u8]) -> bool {
    let Some(target_bits) = cast
        .child_by_field_name("type")
        .and_then(|t| t.utf8_text(source).ok())
        .and_then(unsigned_int_bits)
    else {
        return false;
    };
    let target_max: u128 = if target_bits >= 128 {
        u128::MAX
    } else {
        (1u128 << target_bits) - 1
    };
    let Some(value) = cast.child_by_field_name("value") else {
        return false;
    };
    binary_is_modulo_bounded(value, target_max, source)
}

/// True when `value` (parens transparent) is `x % N` whose remainder always fits
/// an unsigned target of maximum `target_max`: `N` is an integer literal with
/// `N - 1 <= target_max`, and the dividend `x` is provably non-negative per
/// [`expr_is_provably_nonneg`] (so `x % N` lands in `[0, N - 1]`). The shared
/// bound check behind both the inline `(x % N) as uT` exemption
/// ([`cast_operand_is_modulo_bounded`]) and its let-bound form
/// ([`cast_operand_is_modulo_bounded_via_binding`]).
fn binary_is_modulo_bounded(value: Node, target_max: u128, source: &[u8]) -> bool {
    let mut value = value;
    while value.kind() == "parenthesized_expression" {
        let Some(inner) = value.named_child(0) else {
            return false;
        };
        value = inner;
    }
    if value.kind() != "binary_expression" {
        return false;
    }
    if value
        .child_by_field_name("operator")
        .and_then(|op| op.utf8_text(source).ok())
        != Some("%")
    {
        return false;
    }
    let (Some(left), Some(right)) = (
        value.child_by_field_name("left"),
        value.child_by_field_name("right"),
    ) else {
        return false;
    };
    // `x % N` (non-negative `x`) yields a value in `[0, N - 1]`; it fits when
    // `N - 1 <= target_max`. A zero or absent literal rejects via `checked_sub` /
    // `parse_int_literal`, keeping `(x % N)` with an unparsed bound flagged.
    if right.kind() != "integer_literal" {
        return false;
    }
    if !parse_int_literal(right, source)
        .and_then(|n| n.checked_sub(1))
        .is_some_and(|remainder_max| remainder_max <= target_max)
    {
        return false;
    }
    expr_is_provably_nonneg(left, source)
}

/// True when `cast` (a `type_cast_expression`) narrows `v as uT` where `v` is an
/// identifier last bound by `let v = x % N` whose modulo bound carries to the
/// cast site — the let-bound counterpart of [`cast_operand_is_modulo_bounded`]'s
/// inline `(x % N) as uT`.
///
/// Digit-extraction loops (itoa's `enc_*`) stage the remainder in a local before
/// casting it: `let quad = remain % 1_00_00; divmod100(quad as u32)`. The cast's
/// operand is then a bare `identifier`, so the inline modulo check cannot see the
/// bound. This predicate resolves the identifier to its innermost preceding `let`
/// in the enclosing block and applies the SAME bound check
/// ([`binary_is_modulo_bounded`]): unsigned target, literal `N` with
/// `N - 1 <= uT::MAX`, provably non-negative dividend.
///
/// Soundness adds a binding guard on top of that bound: the modulo value proves
/// `v`'s value only while `v` is unchanged between the `let` and the cast. A
/// shadowing re-`let v` or a reassignment (`v = …` / `v += …`) in between
/// replaces it, so [`name_rebound_in_range`] vetoes the exemption and the cast
/// stays flagged. A later non-modulo `let v = big` therefore keeps flagging.
///
/// Only the innermost enclosing `block` is scanned, so a `let` in an outer scope
/// is left unresolved (a conservative false-negative, never an unsound
/// exemption). Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`.
pub fn cast_operand_is_modulo_bounded_via_binding(cast: Node, source: &[u8]) -> bool {
    let Some(value) = cast.child_by_field_name("value") else {
        return false;
    };
    if value.kind() != "identifier" {
        return false;
    }
    let Ok(name) = value.utf8_text(source) else {
        return false;
    };
    let Some(target_bits) = cast
        .child_by_field_name("type")
        .and_then(|t| t.utf8_text(source).ok())
        .and_then(unsigned_int_bits)
    else {
        return false;
    };
    let target_max: u128 = if target_bits >= 128 {
        u128::MAX
    } else {
        (1u128 << target_bits) - 1
    };

    let Some((block, cast_stmt)) = enclosing_block_statement(cast) else {
        return false;
    };
    let cast_start = cast.start_byte();
    let mut cursor = block.walk();
    for stmt in block.named_children(&mut cursor) {
        if stmt.id() == cast_stmt.id() {
            break;
        }
        if stmt.kind() != "let_declaration" {
            continue;
        }
        let binds_name = stmt
            .child_by_field_name("pattern")
            .is_some_and(|p| pattern_contains_identifier(p, name, source));
        if !binds_name {
            continue;
        }
        let Some(bound_value) = stmt.child_by_field_name("value") else {
            continue;
        };
        if !binary_is_modulo_bounded(bound_value, target_max, source) {
            continue;
        }
        // The modulo bound proves `name`'s value only while it stays unchanged
        // between the `let` and the cast; a shadowing re-`let` or reassignment in
        // `(let, cast)` replaces it and vetoes the exemption.
        if !name_rebound_in_range(block, stmt.end_byte(), cast_start, name, source) {
            return true;
        }
    }
    false
}

/// True when `node`'s value is provably `>= 0` from the AST alone (no type
/// inference) — the non-negativity [`cast_operand_is_modulo_bounded`] needs to
/// make an unsigned `%` lossless.
///
/// Proven shapes:
/// - an `integer_literal` (literals carry no sign; a leading `-` is a separate
///   `unary_expression`);
/// - an `identifier` whose same-file binding/parameter is annotated with an
///   unsigned integer type (`u8`..`u128`/`usize`);
/// - `a % b`, non-negative when `a` is (the remainder follows the dividend's sign);
/// - `a & b`, non-negative when either operand is (a bitwise AND clears the sign
///   bit unless both operands have it set).
///
/// Any `parenthesized_expression` layers are transparent. Any other shape
/// (`a + b`, a method call, an unannotated local) is left unproven, so the caller
/// keeps flagging it.
fn expr_is_provably_nonneg(node: Node, source: &[u8]) -> bool {
    let mut node = node;
    while node.kind() == "parenthesized_expression" {
        let Some(inner) = node.named_child(0) else {
            return false;
        };
        node = inner;
    }
    match node.kind() {
        "integer_literal" => true,
        "identifier" => node
            .utf8_text(source)
            .ok()
            .and_then(|name| find_identifier_type(node, name, source))
            .and_then(|t| unsigned_int_bits(&t))
            .is_some(),
        "binary_expression" => {
            let op = node
                .child_by_field_name("operator")
                .and_then(|op| op.utf8_text(source).ok());
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");
            match op {
                Some("%") => left.is_some_and(|l| expr_is_provably_nonneg(l, source)),
                Some("&") => {
                    left.is_some_and(|l| expr_is_provably_nonneg(l, source))
                        || right.is_some_and(|r| expr_is_provably_nonneg(r, source))
                }
                _ => false,
            }
        }
        _ => false,
    }
}

/// True when `cast` (a `type_cast_expression`) narrows `<recv>.min(<bound>) as uT`
/// where the explicit `.min()` clamp proves the value fits the unsigned target —
/// the saturation pattern `now.elapsed().as_nanos().min(u64::MAX as u128) as u64`.
///
/// `Ord::min(self, other: Self)` returns `min(recv, bound)`, a value that is at
/// most `bound`. The cast is lossless when the result lands in `[0, uT::MAX]`:
///
/// - **upper bound** — `min(recv, bound) <= bound`, so a `bound` value `<= uT::MAX`
///   caps the result within range;
/// - **lower bound** — `.min()` only caps the upper side, so the result can equal a
///   negative `recv`; `(-1i64).min(255) as u8` wraps to `255`. The result is `>= 0`
///   only when `recv` is non-negative. Two structural proofs establish that:
///   1. the `bound` is itself an **unsigned-typed** expression (`u64::MAX`, or
///      `<expr> as u128`). Because `min` takes `other: Self`, the receiver shares
///      that unsigned type, so `recv >= 0`;
///   2. failing that, a bare integer-literal `bound` carries no type, so the
///      receiver must be proven non-negative on its own via
///      [`expr_is_provably_nonneg`].
///
/// The target must be **unsigned** (`u8`..`u128`/`usize`); a signed target's value
/// could fall below `T::MIN`, which `.min()` cannot rule out, so it stays flagged.
/// A non-`min` method, a wrong-direction `.max()`, a `bound` exceeding `uT::MAX`,
/// an unparsable bound, or an unprovable receiver sign all keep flagging. Any
/// `parenthesized_expression` layers around the operand are transparent.
///
/// Like the rest of this AST-heuristic family (`.len()` is `usize`, `%`/`&` are
/// integer semantics), proof 1 assumes `.min` is `Ord::min`/`{f32,f64}::min`; a
/// user type that shadows `min` with a divergent signature is out of contract.
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`.
pub fn cast_operand_is_min_clamped(cast: Node, source: &[u8]) -> bool {
    // Unsigned target only: the result's lower bound is 0 exactly when the receiver
    // is non-negative; a signed target whose value could be `< T::MIN` is never
    // exempt because `.min()` clamps only the upper side.
    let Some(target_bits) = cast
        .child_by_field_name("type")
        .and_then(|t| t.utf8_text(source).ok())
        .and_then(unsigned_int_bits)
    else {
        return false;
    };
    let target_max: u128 = if target_bits >= 128 {
        u128::MAX
    } else {
        (1u128 << target_bits) - 1
    };

    let Some(mut value) = cast.child_by_field_name("value") else {
        return false;
    };
    while value.kind() == "parenthesized_expression" {
        let Some(inner) = value.named_child(0) else {
            return false;
        };
        value = inner;
    }
    // The operand must be a method call `<recv>.min(<bound>)`.
    if value.kind() != "call_expression" {
        return false;
    }
    let Some(function) = value.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    if function
        .child_by_field_name("field")
        .and_then(|f| f.utf8_text(source).ok())
        != Some("min")
    {
        return false;
    }
    let Some(args) = value.child_by_field_name("arguments") else {
        return false;
    };
    let mut arg_cursor = args.walk();
    let bound_args: Vec<Node> = args.named_children(&mut arg_cursor).collect();
    let [bound] = bound_args.as_slice() else {
        return false;
    };
    let bound = *bound;

    // Proof 1: an unsigned-typed bound forces (via `min`'s `other: Self`) the
    // receiver to share that unsigned type, so `min(recv, bound) >= 0`; the bound's
    // value caps it above.
    if min_bound_unsigned_typed_value(bound, source).is_some_and(|v| v <= target_max) {
        return true;
    }
    // Proof 2: a bare integer-literal bound proves nothing about the receiver's
    // sign, so exempt only when the receiver is itself provably non-negative.
    if bound.kind() == "integer_literal"
        && function
            .child_by_field_name("value")
            .is_some_and(|recv| expr_is_provably_nonneg(recv, source))
    {
        return parse_int_literal(bound, source).is_some_and(|v| v <= target_max);
    }
    false
}

/// The value of a `.min()` clamp bound when its *type* is a provably unsigned
/// integer and its value is statically known — the proof that the bound's receiver
/// (sharing the type through `Ord::min`'s `Self`) is non-negative. Recognized
/// shapes (parens transparent):
/// - `<utype>::MAX` — value is that unsigned type's maximum;
/// - `<inner> as <utype>` — an unsigned-target cast fixes the type; the value is
///   `<inner>`'s static const/literal value, accepted only when it fits `<utype>`
///   so the cast cannot truncate it (the canonical `u64::MAX as u128` widen).
///
/// Returns `None` for a bare (untyped) literal or any other shape — those cannot
/// prove the receiver's sign here.
fn min_bound_unsigned_typed_value(node: Node, source: &[u8]) -> Option<u128> {
    let mut node = node;
    while node.kind() == "parenthesized_expression" {
        node = node.named_child(0)?;
    }
    match node.kind() {
        "scoped_identifier" => static_int_value(node, source),
        "type_cast_expression" => {
            let utype_bits = node
                .child_by_field_name("type")
                .and_then(|t| t.utf8_text(source).ok())
                .and_then(unsigned_int_bits)?;
            let utype_max: u128 = if utype_bits >= 128 {
                u128::MAX
            } else {
                (1u128 << utype_bits) - 1
            };
            let v = static_int_value(node.child_by_field_name("value")?, source)?;
            (v <= utype_max).then_some(v)
        }
        _ => None,
    }
}

/// The statically-known value of an integer literal or a `<utype>::MAX` scoped
/// constant, irrespective of its type. Parens transparent. `None` for any other
/// shape.
fn static_int_value(node: Node, source: &[u8]) -> Option<u128> {
    let mut node = node;
    while node.kind() == "parenthesized_expression" {
        node = node.named_child(0)?;
    }
    match node.kind() {
        "integer_literal" => parse_int_literal(node, source),
        "scoped_identifier" => {
            if node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                != Some("MAX")
            {
                return None;
            }
            let bits = node
                .child_by_field_name("path")
                .and_then(|p| p.utf8_text(source).ok())
                .and_then(unsigned_int_bits)?;
            Some(if bits >= 128 {
                u128::MAX
            } else {
                (1u128 << bits) - 1
            })
        }
        _ => None,
    }
}

/// True when `cast` (a `type_cast_expression`) is the argument of a
/// `from_bits` call — `f32::from_bits(p as u32)`, `f64::from_bits(x as u64)`,
/// or any `<T>::from_bits(..)`.
///
/// `from_bits` reinterprets an integer's raw bits as another type (a float's
/// IEEE-754 encoding, a bitflags set, …); its argument is a deliberate bit
/// pattern, not a numeric quantity. The same-width signed↔unsigned `as` cast
/// that adapts the operand to `from_bits`'s parameter type (e.g. the `i32` the
/// x86 `_mm_extract_ps` intrinsic returns, cast to the `u32` `f32::from_bits`
/// expects) preserves every bit and is the only correct conversion: a
/// `try_from` would reject negative bit patterns. Both numeric-cast rules treat
/// such a cast as lossless.
///
/// A single layer of `parenthesized_expression` between the cast and the
/// argument list is transparent (`from_bits((p as u32))`). The match keys on
/// the call's `function` field's last path segment being `from_bits`, so it
/// covers any receiver type without enumerating them.
pub fn cast_feeds_from_bits(cast: Node, source: &[u8]) -> bool {
    let mut current = cast;
    while let Some(parent) = current.parent() {
        if parent.kind() == "parenthesized_expression" {
            current = parent;
            continue;
        }
        if parent.kind() == "arguments" {
            let Some(call) = parent.parent().filter(|c| c.kind() == "call_expression") else {
                return false;
            };
            let Some(function) = call.child_by_field_name("function") else {
                return false;
            };
            return call_function_last_segment(function, source) == Some("from_bits");
        }
        return false;
    }
    false
}

/// True when `cast` is the *value* argument of a `ptr::write` / `write_unaligned`
/// / `write_volatile` call whose *destination* argument is a raw pointer cast to
/// a fixed-width integer of the SAME width as the cast's target.
///
/// `write_unaligned(addr as *mut u16, value as u16)` is the idiomatic "store the
/// low N bytes at this address" operation: the destination pointer's pointee type
/// fixes the store width, and the `as uN` on the value IS that width truncation —
/// exactly the lossy cast the typed store requires. This is the relocation-patch
/// pattern in JIT linkers (`Abs1`/`Abs2`/`Abs4` → `as u8`/`as u16`/`as u32`),
/// where the destination width is the structural proof of intent, not a name. A
/// `try_into()` is wrong here: the truncation to the store width is deliberate.
/// Both numeric-cast rules treat such a cast as lossless.
///
/// The match keys on the destination argument's pointee width equalling the
/// value cast's target width, so a *mismatched* store (`addr as *mut u8`, `value
/// as u16`) is NOT exempt, and an ordinary lossy cast with no pointer-write feed
/// still flags. The call's last path segment must name one of the three write
/// intrinsics, covering any receiver path (`ptr::write_unaligned`, `core::ptr::
/// write`, …) without enumerating them. A single layer of
/// `parenthesized_expression` between the cast and the argument list is
/// transparent.
pub fn cast_feeds_sized_pointer_write(cast: Node, source: &[u8]) -> bool {
    let mut current = cast;
    while let Some(parent) = current.parent() {
        if parent.kind() == "parenthesized_expression" {
            current = parent;
            continue;
        }
        if parent.kind() != "arguments" {
            return false;
        }
        let Some(call) = parent.parent().filter(|c| c.kind() == "call_expression") else {
            return false;
        };
        let Some(function) = call.child_by_field_name("function") else {
            return false;
        };
        if !matches!(
            call_function_last_segment(function, source),
            Some("write" | "write_unaligned" | "write_volatile")
        ) {
            return false;
        }
        // The value cast must not itself be the destination pointer argument.
        let mut cursor = parent.walk();
        let mut args = parent.named_children(&mut cursor);
        let Some(dest) = args.next() else {
            return false;
        };
        if dest == current {
            return false;
        }
        let Some((_, value_bits)) = cast_target_int_kind(cast, source) else {
            return false;
        };
        return pointer_cast_pointee_bits(dest, source) == Some(value_bits);
    }
    false
}

/// True when the operand of `cast` (a `type_cast_expression`) is a raw pointer,
/// recognised structurally without type inference.
///
/// A raw-pointer-to-integer cast (`ptr as usize`, `buf.as_ptr() as u32`) is
/// categorically not a lossy numeric conversion: no `From`/`TryFrom` impl exists
/// for `*const T`/`*mut T` to an integer, so `as` is the only conversion, and the
/// rules' `try_from`/`from` remediation does not compile. It is the standard
/// embedded idiom for handing a memory-mapped register / DMA buffer address to a
/// hardware register. Both numeric-cast rules treat such a cast as out of scope.
///
/// The recognised operand shapes, all derived from the cast TARGET/inner-cast
/// TYPE which is present in the AST (no source-type inference, no name allowlist):
///
/// - the operand is itself a `type_cast_expression` whose target is a
///   `pointer_type` — `executor as *const _`, `&mut self.table as *mut u32` — so
///   the value handed to the outer `as <int>` is provably a raw pointer;
/// - the operand is a method call `.as_ptr()` / `.as_mut_ptr()`, the standard way
///   to obtain a raw pointer from a slice/array/`Vec`/`UnsafeCell`/register block;
/// - the operand is a `ptr::null()` / `ptr::null_mut()` call (any path:
///   `core::ptr::null`, `std::ptr::null_mut`, turbofished `null::<T>()`).
///
/// A single layer of `parenthesized_expression` around the operand is
/// transparent. A plain integer operand (`len as u32`) is not a pointer and is
/// left to the rule's numeric-truncation logic.
pub fn cast_operand_is_raw_pointer(cast: Node, source: &[u8]) -> bool {
    let Some(mut value) = cast.child_by_field_name("value") else {
        return false;
    };
    while value.kind() == "parenthesized_expression" {
        let Some(inner) = value.named_child(0) else {
            return false;
        };
        value = inner;
    }
    match value.kind() {
        // `<expr> as *const T` / `<expr> as *mut T`: the inner cast yields a raw
        // pointer, so the outer `as <int>` is a pointer-to-integer cast.
        "type_cast_expression" => value
            .child_by_field_name("type")
            .is_some_and(|t| t.kind() == "pointer_type"),
        // `recv.as_ptr()` / `recv.as_mut_ptr()` / `ptr::null()` / `null_mut()`.
        "call_expression" => value
            .child_by_field_name("function")
            .and_then(|f| pointer_call_segment(f, source))
            .is_some_and(|seg| {
                matches!(seg, "as_ptr" | "as_mut_ptr" | "null" | "null_mut")
            }),
        _ => false,
    }
}

/// The last path/field segment naming a call's callee, also unwrapping a
/// `generic_function` (a turbofished call like `null::<u8>()`) to its underlying
/// callee. Returns `None` for callee shapes with no simple trailing name.
fn pointer_call_segment<'a>(function: Node, source: &'a [u8]) -> Option<&'a str> {
    let callee = if function.kind() == "generic_function" {
        function.child_by_field_name("function")?
    } else {
        function
    };
    call_function_last_segment(callee, source)
}

/// If `node` is `<expr> as *[const|mut] <int>` (a `type_cast_expression` whose
/// target is a `pointer_type` over a fixed-width integer), the pointee's bit
/// width; otherwise `None`.
fn pointer_cast_pointee_bits(node: Node, source: &[u8]) -> Option<u16> {
    if node.kind() != "type_cast_expression" {
        return None;
    }
    let pointer_type = node.child_by_field_name("type").filter(|t| t.kind() == "pointer_type")?;
    let pointee = pointer_type.child_by_field_name("type")?;
    let text = pointee.utf8_text(source).ok()?.trim();
    fixed_width_int_kind(text).map(|(_, bits)| bits)
}

/// True when `cast` (a `type_cast_expression`) is a direct argument of an x86/
/// x86-64 SIMD intrinsic call (`_mm_*`, `_mm256_*`, `_mm512_*`) and is a
/// same-width signed↔unsigned bit reinterpretation.
///
/// Intel's SIMD intrinsic headers type integer lanes as *signed* integers (the C
/// ABI convention), so `core::arch` mirrors that: `_mm_set_epi64x` takes `i64`
/// lanes, `_mm_set1_epi32` takes `i32`, etc. A programmer holding `u64` bit
/// patterns (splat masks, hex bit-select constants) must cast them to the
/// intrinsic's signed lane type. That `u64 as i64` is a same-width
/// reinterpretation — every bit is preserved, only the sign interpretation
/// changes — and `as` is the only correct tool: a `try_from` would reject bit
/// patterns above `i64::MAX`. Both numeric-cast rules treat such a cast as
/// lossless.
///
/// "Same width" is verified two ways so the unresolvable-source case (the cast
/// of a `u64`-returning call's result, e.g. `splat_byte(b) as i64`) is covered:
///
/// - if the operand's source type resolves to a fixed-width integer, it must be
///   the SAME width and the OPPOSITE signedness of the target (`u64 as i64`,
///   `i32 as u32`, …); a narrowing into the lane type (`u64 as i32`) is rejected
///   because the widths differ;
/// - if the source is unresolvable, the intrinsic's lane width — parsed from the
///   `epiN`/`epuN` token in its name (`set_epi64x` → 64, `set1_epi32` → 32) —
///   must equal the target's width, confirming the target is the genuine lane
///   type rather than a narrower one cast into the call.
///
/// A single layer of `parenthesized_expression` between the cast and the
/// argument list is transparent.
pub fn cast_feeds_simd_intrinsic(cast: Node, source: &[u8]) -> bool {
    let Some(callee_segment) = simd_intrinsic_call_segment(cast, source) else {
        return false;
    };
    let Some((target_signed, target_bits)) = cast_target_int_kind(cast, source) else {
        return false;
    };
    match classify_cast_source(cast, source) {
        // Resolved fixed-width source: a genuine same-width signed↔unsigned
        // reinterpretation.
        CastSource::FixedWidth(source_signed, source_bits) => {
            source_bits == target_bits && source_signed != target_signed
        }
        // Resolved to a platform-width (`usize`/`isize`) source: its width is not
        // statically known, so "same width" cannot be proven — never exempt.
        CastSource::PlatformWidth => false,
        // Unresolved source: the target must be the intrinsic's true lane type,
        // i.e. the lane width encoded in the intrinsic name equals the target.
        CastSource::Unresolved => simd_intrinsic_lane_bits(callee_segment) == Some(target_bits),
    }
}

/// The resolution outcome for a cast operand's source type.
enum CastSource {
    /// A fixed-width integer (`true` = signed) of the given bit width.
    FixedWidth(bool, u16),
    /// A platform-width integer (`usize`/`isize`) — width not statically known.
    PlatformWidth,
    /// No locally-visible type (method return, un-annotated binding, …).
    Unresolved,
}

/// If `cast` is a direct argument (through transparent parentheses) of a
/// `call_expression` whose callee's last path segment names an x86 SIMD
/// intrinsic (`_mm_*`, `_mm256_*`, `_mm512_*`), return that segment; otherwise
/// `None`.
fn simd_intrinsic_call_segment<'a>(cast: Node, source: &'a [u8]) -> Option<&'a str> {
    let mut current = cast;
    while let Some(parent) = current.parent() {
        if parent.kind() == "parenthesized_expression" {
            current = parent;
            continue;
        }
        if parent.kind() == "arguments" {
            let call = parent.parent().filter(|c| c.kind() == "call_expression")?;
            let function = call.child_by_field_name("function")?;
            let segment = call_function_last_segment(function, source)?;
            return is_simd_intrinsic_name(segment).then_some(segment);
        }
        return None;
    }
    None
}

/// True if `name` is an x86/x86-64 SIMD intrinsic identifier — one prefixed by a
/// register-width tag (`_mm_`, `_mm256_`, `_mm512_`).
fn is_simd_intrinsic_name(name: &str) -> bool {
    name.starts_with("_mm_") || name.starts_with("_mm256_") || name.starts_with("_mm512_")
}

/// The lane bit width encoded in a SIMD intrinsic name's `epiN`/`epuN` token
/// (`_mm_set_epi64x` → 64, `_mm_set1_epi32` → 32, `_mm256_set1_epu16` → 16), or
/// `None` when the name carries no such lane-width token.
fn simd_intrinsic_lane_bits(name: &str) -> Option<u16> {
    for tag in ["epi", "epu"] {
        if let Some(idx) = name.find(tag) {
            let after = &name[idx + tag.len()..];
            let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
            if let Ok(bits) = digits.parse::<u16>() {
                return Some(bits);
            }
        }
    }
    None
}

/// The (signedness, bit-width) of `cast`'s target type when it is a fixed-width
/// integer (`i8`..`i128`, `u8`..`u128`), or `None` for `usize`/`isize`, floats,
/// and non-numeric targets. Platform-width targets are excluded so "same width"
/// is always a definite comparison.
fn cast_target_int_kind(cast: Node, source: &[u8]) -> Option<(bool, u16)> {
    let target = cast.child_by_field_name("type")?.utf8_text(source).ok()?.trim();
    fixed_width_int_kind(target)
}

/// True when `cast` converts a resolvable integer operand to a float target
/// (`<int> as f32` / `<int> as f64`).
///
/// A lossy integer→float conversion has no `From`/`TryFrom` alternative in std:
/// the trait impls exist only for the lossless pairs (`f64::from(i32)`,
/// `f32::from(i16)`, …); for the lossy pairs (`u64`/`i64`/`u128`/`i128`/`usize`/
/// `isize` → `f64`, and `i32`/`u32` and wider → `f32`) no `From` and no
/// `TryFrom` exists. `as` is then the only conversion the language offers, so a
/// rule suggesting `try_into()` / `From::from(x)` would emit an impossible fix.
///
/// The operand's source type must be a resolvable integer (a fixed-width
/// integer or `usize`/`isize`, seen through a deref/borrow or as a typed
/// container's element). An unresolved operand returns `false` — its kind is not
/// proven to be an integer, so the caller's own unresolved-operand handling
/// applies. A `char`/`bool` operand is not an integer here, and a float→float
/// narrowing (`f64 as f32`) has a float source, so neither is exempted.
pub fn cast_is_int_to_float(cast: Node, source: &[u8]) -> bool {
    let Some(target) =
        cast.child_by_field_name("type").and_then(|t| t.utf8_text(source).ok()).map(str::trim)
    else {
        return false;
    };
    if target != "f32" && target != "f64" {
        return false;
    }
    matches!(
        classify_cast_source(cast, source),
        CastSource::FixedWidth(..) | CastSource::PlatformWidth
    )
}

/// Classify `cast`'s operand source type when locally visible: a
/// bare/dereferenced identifier with a local annotation, or an index into a
/// locally-typed integer container. A resolved `usize`/`isize` is reported as
/// [`CastSource::PlatformWidth`] (width unknown), any other unresolved or
/// non-integer operand as [`CastSource::Unresolved`].
fn classify_cast_source(cast: Node, source: &[u8]) -> CastSource {
    let Some(value) = cast.child_by_field_name("value") else {
        return CastSource::Unresolved;
    };
    let type_text = if let Some(element_type) = cast_operand_indexed_element_type(cast, source) {
        element_type
    } else {
        let ident = deref_borrow_identifier(value, source).unwrap_or(value);
        if ident.kind() != "identifier" {
            return CastSource::Unresolved;
        }
        let Ok(name) = ident.utf8_text(source) else {
            return CastSource::Unresolved;
        };
        match find_identifier_type(cast, name, source) {
            Some(t) => strip_leading_borrow(&t).to_string(),
            None => return CastSource::Unresolved,
        }
    };
    let t = type_text.trim();
    if t == "usize" || t == "isize" {
        return CastSource::PlatformWidth;
    }
    match fixed_width_int_kind(t) {
        Some((signed, bits)) => CastSource::FixedWidth(signed, bits),
        None => CastSource::Unresolved,
    }
}

/// If `value` is a unary dereference of an identifier (`*x`), return the inner
/// identifier node, peeling one parenthesized wrapper (`(*x)`); otherwise `None`.
fn deref_borrow_identifier<'a>(value: Node<'a>, source: &[u8]) -> Option<Node<'a>> {
    if value.kind() == "parenthesized_expression" {
        return value.named_child(0).and_then(|inner| deref_borrow_identifier(inner, source));
    }
    if value.kind() != "unary_expression" {
        return None;
    }
    let is_deref = value
        .child(0)
        .and_then(|op| op.utf8_text(source).ok())
        .is_some_and(|op| op == "*");
    if !is_deref {
        return None;
    }
    value.named_child(0).filter(|operand| operand.kind() == "identifier")
}

/// Strip a single leading `&` / `&mut` borrow from a type's source text so a
/// dereferenced operand resolves to its referent (`&u16` → `u16`).
fn strip_leading_borrow(type_text: &str) -> &str {
    match type_text.trim_start().strip_prefix('&') {
        Some(rest) => rest.trim_start().strip_prefix("mut ").unwrap_or(rest).trim_start(),
        None => type_text,
    }
}

/// The (signedness, bit-width) of a fixed-width integer type name (`true` for
/// signed), or `None` for `usize`/`isize`, floats, and non-numeric types.
fn fixed_width_int_kind(type_text: &str) -> Option<(bool, u16)> {
    let t = type_text.trim();
    if let Some(bits) = signed_int_bits(t).filter(|_| t != "isize") {
        return Some((true, bits));
    }
    unsigned_int_bits(t).filter(|_| t != "usize").map(|bits| (false, bits))
}

/// The final path segment of a call's `function` node, used to match a call by
/// method/associated-function name regardless of its receiver/path prefix.
/// `f32::from_bits` and `core::f32::from_bits` both yield `from_bits`.
fn call_function_last_segment<'a>(function: Node, source: &'a [u8]) -> Option<&'a str> {
    let name = match function.kind() {
        "scoped_identifier" => function.child_by_field_name("name")?,
        "field_expression" => function.child_by_field_name("field")?,
        "identifier" => function,
        _ => return None,
    };
    name.utf8_text(source).ok()
}

/// The compile-time value of `cast`'s operand when it is an integer or byte
/// literal whose value is statically known, or `None` for any other operand.
///
/// Recognized operands:
/// - an `integer_literal` in decimal, hex (`0x`), octal (`0o`), or binary
///   (`0b`), with optional digit separators (`_`) and an optional integer type
///   suffix (`65u8`, `0xFFi32`);
/// - the same wrapped in a leading unary minus (`-5 as i8`);
/// - a byte literal `b'A'` / `b'\n'` / `b'\x41'`, whose value is the byte.
///
/// Float and `char` literals are excluded: precision/width for those is handled
/// by the float-target machinery and `cast_operand_is_char`. The value is
/// returned as `i128`, wide enough to hold every fixed-width integer literal
/// (including `u128::MAX`) without loss.
///
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`: a literal
/// whose value provably fits the target type's range is a lossless, statically
/// verifiable cast (e.g. `b' ' as i8`, the WinAPI `CHAR` idiom). Each rule pairs
/// this with its target's `[MIN, MAX]` bounds and exempts the cast only when the
/// value is in range; an out-of-range literal stays flagged.
pub fn cast_operand_literal_value(cast: Node, source: &[u8]) -> Option<i128> {
    let value = cast.child_by_field_name("value")?;
    operand_literal_value(value, source)
}

fn operand_literal_value(node: Node, source: &[u8]) -> Option<i128> {
    match node.kind() {
        "integer_literal" => parse_integer_literal(node.utf8_text(source).ok()?),
        "char_literal" => byte_literal_value(node.utf8_text(source).ok()?),
        "unary_expression" => {
            // Only a leading minus negates the inner literal; `!lit` is bitwise
            // and `*ptr`/`&x` are not literals.
            let is_neg = node
                .child(0)
                .and_then(|op| op.utf8_text(source).ok())
                .is_some_and(|op| op == "-");
            if !is_neg {
                return None;
            }
            node.named_child(0)
                .and_then(|inner| operand_literal_value(inner, source))
                .and_then(i128::checked_neg)
        }
        _ => None,
    }
}

/// Parse an `integer_literal`'s text into its value: decimal / `0x` / `0o` /
/// `0b`, with `_` separators and an optional integer type suffix stripped. Only
/// integer suffixes are accepted — a float-suffixed token (`5f32`) parses as a
/// `float_literal`, never reaching here. Returns `None` on overflow of `i128`.
fn parse_integer_literal(text: &str) -> Option<i128> {
    let text = text.trim();
    let (radix, rest) = match text.as_bytes() {
        [b'0', b'x' | b'X', ..] => (16, &text[2..]),
        [b'0', b'o' | b'O', ..] => (8, &text[2..]),
        [b'0', b'b' | b'B', ..] => (2, &text[2..]),
        _ => (10, text),
    };
    let mut digits = String::with_capacity(rest.len());
    for ch in rest.chars() {
        if ch == '_' {
            continue;
        }
        if ch.is_digit(radix) {
            digits.push(ch);
        } else {
            // The first non-digit, non-separator char begins the type suffix
            // (`u8`, `i32`, …); the remainder is the suffix, not part of the value.
            break;
        }
    }
    if digits.is_empty() {
        return None;
    }
    i128::from_str_radix(&digits, radix).ok()
}

/// The value of a byte literal `b'…'` (an ASCII byte, `0..=255`), handling the
/// escapes Rust permits in a byte literal: `\n \r \t \\ \0 \' \"` and `\xNN`.
/// Returns `None` for any text that is not a single-byte literal.
fn byte_literal_value(text: &str) -> Option<i128> {
    let inner = text.strip_prefix("b'")?.strip_suffix('\'')?;
    let value = match inner.as_bytes() {
        [b'\\', b'x', hi, lo] => {
            let pair = [*hi, *lo];
            let hex = std::str::from_utf8(&pair).ok()?;
            u8::from_str_radix(hex, 16).ok()?
        }
        [b'\\', esc] => match esc {
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'\\' => b'\\',
            b'0' => 0,
            b'\'' => b'\'',
            b'"' => b'"',
            _ => return None,
        },
        [b] => *b,
        _ => return None,
    };
    Some(i128::from(value))
}

/// True if `cast` (a `type_cast_expression`) reads the discriminant of a
/// fieldless (C-like) enum — `<enum value> as <integer>`. For such an enum the
/// `as`-cast is the language-blessed way to obtain the discriminant: no
/// `From<Enum> for {integer}` / `TryFrom<Enum> for {integer}` impl exists, so
/// the rules' usual `from`/`try_from` remediations would not compile.
///
/// The operand (the `value` field of the cast) qualifies when it is provably a
/// fieldless-enum value, recognized from the AST without type inference:
///
/// - `self` inside an `impl <Enum>` block whose target `<Enum>` is a fieldless
///   `enum_item` defined in the same file. `self as <integer>` only type-checks
///   when `Self` is a fieldless enum (or a primitive), so the shape is
///   unambiguous; or
/// - a `scoped_identifier` `EnumName::Variant` where `EnumName` is a fieldless
///   `enum_item` in the file; or
/// - a `scoped_identifier` `Path::EnumName::Variant` whose `EnumName` and
///   `Variant` segments are both PascalCase, when the enum is not defined in the
///   file (an imported/external enum such as `lsp_server::ErrorCode::InvalidParams`).
///   The shape `<Type>::<Variant>` is an enum-variant discriminant read; a
///   const reference (`mod::MAX_LEN`) or a function path (`mod::value`) has a
///   non-PascalCase final segment and is excluded; or
/// - an `identifier` whose locally-declared type (a `let`/parameter annotation,
///   resolved by `find_identifier_type`) is either a module-qualified path
///   (`spirv::SourceLanguage`) whose final segment is PascalCase, or a bare
///   single-segment name resolving to a fieldless `enum_item` in the same file.
///   The qualified form — `source_language as u32` where `source_language:
///   spirv::SourceLanguage` — covers imported `#[repr(uN)]` enums whose repr is
///   invisible to the AST: an integer `as`-cast type-checks only when the operand
///   is a numeric primitive, `char`, `bool`, a pointer, or a fieldless enum, so a
///   qualified non-primitive type the cast compiles against is an imported
///   fieldless enum (a struct cannot be `as`-cast), and `as` is the only
///   conversion the language offers (no `From<Enum> for {integer}`). The bare form
///   — `code as i32` where `code: Code` and `Code` is a fieldless in-file enum —
///   is the compiler-required `From<Code> for i32` idiom; the in-file `enum_item`
///   is inspected directly, so a bare name resolving to a data-carrying enum or to
///   a numeric type alias (no matching `enum_item`) stays flagged.
///
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`, which both
/// otherwise flag the cast because a fieldless-enum operand resolves to no
/// numeric type and falls through to their conservative "unknown source" branch.
pub fn cast_operand_is_enum_discriminant(cast: Node, source: &[u8]) -> bool {
    let Some(value) = cast.child_by_field_name("value") else {
        return false;
    };
    match value.kind() {
        "self" => self_enum_is_fieldless(cast, source),
        "scoped_identifier" => scoped_operand_is_enum_discriminant(cast, value, source),
        "identifier" => identifier_operand_is_enum_value(cast, value, source),
        _ => false,
    }
}

/// True if `value` (an `identifier` operand of `cast`) is a binding whose
/// locally-declared type names a fieldless enum read for its discriminant.
///
/// The binding's type is resolved from its `let`/parameter annotation via
/// `find_identifier_type`, then matched in two shapes:
///
/// - a MODULE-QUALIFIED path (`spirv::Decoration`, a type text containing `::`)
///   whose final segment is PascalCase. An integer `as`-cast type-checks only when
///   the operand is a numeric primitive, `char`, `bool`, a pointer, or a fieldless
///   enum, so a qualified non-primitive type the cast compiles against is an
///   imported fieldless enum (its repr is invisible to the AST). The PascalCase
///   gate — shared with the external-variant-path branch — excludes const
///   (`mod::MAX_LEN`) and function (`mod::value`) paths; or
/// - a BARE single-segment name (`Code`) that resolves to a fieldless `enum_item`
///   defined in the same file. This is the `From<Code> for i32 { code as i32 }`
///   idiom: `code` is a fieldless-enum binding and the `as`-cast reads its
///   discriminant — the only way to implement that `From` (`i32::from(code)` would
///   recurse; no `TryFrom` exists for the direction). The in-file `enum_item` is
///   inspected directly, so a bare name resolving to a data-carrying enum is
///   rejected and a bare numeric type alias (no matching `enum_item`) stays
///   flagged.
///
/// A reference operand never reaches here — `&E as u32` does not compile, so a
/// directly-`as`-castable enum binding is never a reference type.
fn identifier_operand_is_enum_value(cast: Node, value: Node, source: &[u8]) -> bool {
    let Ok(name) = value.utf8_text(source) else {
        return false;
    };
    let Some(declared) = find_identifier_type(cast, name, source) else {
        return false;
    };
    let declared = declared.trim();
    match declared.rsplit_once("::") {
        // Module-qualified path (`spirv::SourceLanguage`): an imported enum whose
        // repr the AST cannot see. A PascalCase final segment confirms it names a
        // type, excluding const (`mod::MAX_LEN`) and function (`mod::value`) paths.
        Some((_, type_name)) => is_pascal_case(type_name),
        // Bare single-segment name (`Code`): resolve it in-file. It qualifies only
        // when it names a fieldless `enum_item` defined here — then the binding is a
        // fieldless-enum value and `code as i32` is the discriminant-read idiom (the
        // only way to write `From<Code> for i32`). A data-carrying enum, or a name
        // with no in-file `enum_item` (e.g. a numeric type alias), stays flagged.
        None => find_enum_item(cast, declared, source).is_some_and(enum_is_fieldless),
    }
}

/// True if `cast` (a `type_cast_expression`) reads a struct field typed as a
/// `#[repr(intN)]` fieldless enum and casts it to `target`, an integer wide
/// enough to hold that repr — `<receiver>.<field> as <int>` where `field`'s
/// declared type is such an enum defined in the same file. `#[repr(uN)]`
/// guarantees every discriminant fits `uN`, so the cast to `uN` (or wider, with
/// the same or compatible signedness) is lossless and total; `as` is also the
/// only conversion the language offers there (no `From`/`TryFrom<Enum> for
/// {integer}`), so the rules' usual remediations would not compile.
///
/// Resolution is purely structural (no type inference). The receiver of the
/// field access is resolved to a struct name in two cases:
///
/// - `self.<field>` inside an `impl <Struct>` block whose target `<Struct>` is a
///   `struct_item` defined in the same file; or
/// - `<binding>.<field>` where `<binding>` is a local/parameter annotated with a
///   `<Struct>` type (resolved via `find_identifier_type`), the reference/`mut`
///   prefix peeled off.
///
/// The struct's in-file definition is then searched for a `field_declaration`
/// named `<field>`; its declared type must name a fieldless `enum_item` carrying
/// `#[repr(intN)]`, and `target` must be wide enough (unsigned repr `uN` →
/// unsigned/signed target of ≥ N bits with the signed target one bit wider when
/// equal width would not fit, mirroring the numeric-fit logic). A wider-int
/// field cast to a narrower int (`u16` field `as u8`) is NOT exempted — only a
/// repr-enum field is.
///
/// LIMITATION: the receiver must be a directly-annotated binding or `self`;
/// method-return receivers, nested field chains (`a.b.field`), and bindings whose
/// type is inferred are not resolved and fall through to the conservative branch.
/// The struct and enum are matched by bare name anywhere in the file (modules are
/// not scoped), like `cast_operand_is_enum_discriminant`; a name collision can
/// only suppress a cast, never wrongly flag one, so it stays on the FP-safe side.
///
/// Shared by `rust-no-lossy-as-cast` and `rust-no-as-numeric-cast`.
pub fn cast_operand_is_repr_enum_field(cast: Node, source: &[u8], target: &str) -> bool {
    let Some(target_repr) = repr_int_width(target) else {
        return false;
    };
    let Some(value) = cast.child_by_field_name("value") else {
        return false;
    };
    if value.kind() != "field_expression" {
        return false;
    }
    let Some(field) = value
        .child_by_field_name("field")
        .and_then(|f| f.utf8_text(source).ok())
    else {
        return false;
    };
    let Some(receiver) = value.child_by_field_name("value") else {
        return false;
    };
    let Some(struct_name) = field_receiver_struct_name(cast, receiver, source) else {
        return false;
    };
    let Some(struct_item) = find_struct_item(cast, &struct_name, source) else {
        return false;
    };
    let Some(field_type) = struct_field_type(struct_item, field, source) else {
        return false;
    };
    let Some(enum_item) = find_enum_item(cast, &field_type, source) else {
        return false;
    };
    if !enum_is_fieldless(enum_item) {
        return false;
    }
    let Some(repr) = enum_repr_int_width(enum_item, source) else {
        return false;
    };
    repr_fits(repr, target_repr)
}

/// The (signedness, bit-width) of an integer type name (`u8`, `i32`, `usize`),
/// or `None` for any non-integer (`f32`, a custom type).
fn repr_int_width(type_text: &str) -> Option<(bool, u16)> {
    let (signed, bits) = match type_text.trim() {
        "u8" => (false, 8),
        "u16" => (false, 16),
        "u32" => (false, 32),
        "u64" => (false, 64),
        "u128" => (false, 128),
        "usize" => (false, usize::BITS as u16),
        "i8" => (true, 8),
        "i16" => (true, 16),
        "i32" => (true, 32),
        "i64" => (true, 64),
        "i128" => (true, 128),
        "isize" => (true, usize::BITS as u16),
        _ => return None,
    };
    Some((signed, bits))
}

/// True if every value of a `repr`-typed integer is representable in `target`.
/// An unsigned source needs an unsigned target of ≥ its width, or a signed
/// target strictly wider (one extra bit for the sign). A signed source needs a
/// signed target of ≥ its width (a signed value never fits an unsigned target).
fn repr_fits(repr: (bool, u16), target: (bool, u16)) -> bool {
    let (repr_signed, repr_bits) = repr;
    let (target_signed, target_bits) = target;
    match (repr_signed, target_signed) {
        (false, false) => target_bits >= repr_bits,
        (false, true) => target_bits > repr_bits,
        (true, true) => target_bits >= repr_bits,
        (true, false) => false,
    }
}

/// Resolve the struct name a field-access receiver refers to, structurally:
/// `self` → the enclosing `impl <Struct>` target, or an annotated binding's
/// declared type with any `&`/`&mut` prefix peeled. Returns `None` for receivers
/// that need type inference (method calls, nested fields, inferred bindings).
fn field_receiver_struct_name(cast: Node, receiver: Node, source: &[u8]) -> Option<String> {
    match receiver.kind() {
        "self" => enclosing_impl_type_name(cast, source),
        "identifier" => {
            let name = receiver.utf8_text(source).ok()?;
            let ty = find_identifier_type(cast, name, source)?;
            Some(strip_reference_prefix(&ty).to_string())
        }
        _ => None,
    }
}

/// The `type_identifier` name of `node`'s nearest enclosing `impl_item`, or
/// `None` if the target is not a plain named type.
fn enclosing_impl_type_name(node: Node, source: &[u8]) -> Option<String> {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "impl_item" {
            return ancestor
                .child_by_field_name("type")
                .filter(|target| target.kind() == "type_identifier")
                .and_then(|target| target.utf8_text(source).ok())
                .map(str::to_string);
        }
        current = ancestor.parent();
    }
    None
}

/// Peel a leading `&`/`&mut`/`mut` from a type's source text, leaving the inner
/// type name (`&DisposeOp` → `DisposeOp`).
fn strip_reference_prefix(type_text: &str) -> &str {
    let mut t = type_text.trim();
    loop {
        let stripped = t
            .strip_prefix('&')
            .map(str::trim_start)
            .map(|s| s.strip_prefix("mut").map_or(s, str::trim_start));
        match stripped {
            Some(rest) if rest != t => t = rest,
            _ => return t,
        }
    }
}

/// The first `struct_item` named `name` in the file containing `node`, or `None`.
fn find_struct_item<'a>(node: Node<'a>, name: &str, source: &[u8]) -> Option<Node<'a>> {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        if current.kind() == "struct_item"
            && current
                .child_by_field_name("name")
                .and_then(|name_node| name_node.utf8_text(source).ok())
                == Some(name)
        {
            return Some(current);
        }
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    None
}

/// The declared type-name text of the field `field` in `struct_item`, when that
/// field's type is a plain `type_identifier`, or `None`.
fn struct_field_type(struct_item: Node, field: &str, source: &[u8]) -> Option<String> {
    let body = struct_item.child_by_field_name("body")?;
    let mut cursor = body.walk();
    for decl in body.named_children(&mut cursor) {
        if decl.kind() != "field_declaration" {
            continue;
        }
        if decl
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            != Some(field)
        {
            continue;
        }
        let type_node = decl.child_by_field_name("type")?;
        if type_node.kind() != "type_identifier" {
            return None;
        }
        return type_node.utf8_text(source).ok().map(str::to_string);
    }
    None
}

/// True when `impl_item`'s `Drop` target struct, defined in the same file,
/// declares `field` as a reference type (`&T` / `&'a T` / `&mut T`).
///
/// The lock target of `self.<field>.lock()` then lives *outside* `self`: the
/// struct merely borrows it, so a `Drop` acquiring it cannot self-deadlock on a
/// lock the struct itself holds. Owned mutex fields (`Mutex<T>`,
/// `Arc<Mutex<T>>`, parsed as `generic_type`, not `reference_type`) return
/// `false` and stay flagged.
///
/// Resolution is purely structural: the bare struct name is read from the
/// `impl`'s `type` field (handling both a plain `type_identifier` and a
/// generic `Foo<'a>`), the matching `struct_item` is searched in the same file,
/// and its `field_declaration` for `field` is inspected. If the struct is not
/// in the file or the field is absent, returns `false` (fail-closed: still
/// flag), since comply analyses one file at a time.
///
/// Used by `rust-drop-calls-self-lock`.
pub fn drop_impl_field_is_reference(impl_item: Node, field: &str, source: &[u8]) -> bool {
    let Some(struct_name) = impl_type_struct_name(impl_item, source) else {
        return false;
    };
    let Some(struct_item) = find_struct_item(impl_item, &struct_name, source) else {
        return false;
    };
    field_declaration_is_reference(struct_item, field, source)
}

/// The bare struct name from an `impl_item`'s `type` field: the text of a plain
/// `type_identifier`, or the leading `type_identifier` of a `generic_type`
/// (`UsageScope<'a>` → `UsageScope`). `None` for any other target shape.
fn impl_type_struct_name(impl_item: Node, source: &[u8]) -> Option<String> {
    let type_node = impl_item.child_by_field_name("type")?;
    let name_node = match type_node.kind() {
        "type_identifier" => type_node,
        "generic_type" => type_node
            .child_by_field_name("type")
            .filter(|t| t.kind() == "type_identifier")?,
        _ => return None,
    };
    name_node.utf8_text(source).ok().map(str::to_string)
}

/// True when `field`'s `field_declaration` in `struct_item` has a
/// `reference_type` declared type.
fn field_declaration_is_reference(struct_item: Node, field: &str, source: &[u8]) -> bool {
    let Some(body) = struct_item.child_by_field_name("body") else {
        return false;
    };
    let mut cursor = body.walk();
    for decl in body.named_children(&mut cursor) {
        if decl.kind() != "field_declaration" {
            continue;
        }
        if decl
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            != Some(field)
        {
            continue;
        }
        return decl
            .child_by_field_name("type")
            .is_some_and(|t| t.kind() == "reference_type");
    }
    false
}

/// The (signedness, bit-width) of an `enum_item`'s `#[repr(intN)]` attribute, or
/// `None` if it has no integer `repr` (a `#[repr(C)]` or no `repr` at all has no
/// guaranteed discriminant width).
fn enum_repr_int_width(enum_item: Node, source: &[u8]) -> Option<(bool, u16)> {
    let mut sibling = enum_item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" | "block_comment" | "attribute_item" => {
                if s.kind() == "attribute_item"
                    && let Some(repr) = repr_attribute_int(s, source)
                {
                    return Some(repr);
                }
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    None
}

/// If `attribute_item` is a `#[repr(...)]` whose arguments name an integer type
/// (`u8`, `i32`, …), return that type's (signedness, bit-width). Other reprs
/// (`C`, `transparent`, `packed`) yield `None`.
fn repr_attribute_int(attribute_item: Node, source: &[u8]) -> Option<(bool, u16)> {
    let mut item_cursor = attribute_item.walk();
    let attribute = attribute_item
        .children(&mut item_cursor)
        .find(|child| child.kind() == "attribute")?;
    let path = attribute.named_child(0)?;
    if path.utf8_text(source) != Ok("repr") {
        return None;
    }
    let token_tree = attribute.child_by_field_name("arguments")?;
    let text = token_tree.utf8_text(source).ok()?;
    let inner = text.trim().trim_start_matches('(').trim_end_matches(')');
    inner.split(',').find_map(|tok| repr_int_width(tok.trim()))
}

/// True if the `scoped_identifier` operand `value` of `cast` reads a fieldless
/// enum's discriminant. Resolves the enum in-file when possible; otherwise falls
/// back to the `<EnumType>::<Variant>` shape heuristic for imported enums.
fn scoped_operand_is_enum_discriminant(cast: Node, value: Node, source: &[u8]) -> bool {
    let Some(path) = value.child_by_field_name("path") else {
        return false;
    };
    if let Ok(enum_name) = path.utf8_text(source)
        && let Some(enum_item) = find_enum_item(cast, enum_name, source)
    {
        // The enum is defined in this file: trust its variants directly. A
        // data-carrying enum has no discriminant-`as` semantics, so it must
        // stay flagged rather than fall through to the shape heuristic.
        return enum_is_fieldless(enum_item);
    }
    // Imported/external enum: no `enum_item` to inspect. `<Type>::<Variant> as int`
    // is the discriminant-read idiom; require both the enum-type segment and the
    // variant segment to be PascalCase to exclude const (`mod::MAX`) and function
    // (`mod::value`) paths. Without type info this also exempts a PascalCase
    // associated const (`Wrapper::Default as i32`) — an accepted blind spot, as
    // avoiding the discriminant-read false positive outweighs that rare miss.
    let variant_is_pascal = value
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(source).ok())
        .is_some_and(is_pascal_case);
    let enum_type_is_pascal = final_segment(path, source).is_some_and(is_pascal_case);
    variant_is_pascal && enum_type_is_pascal
}

/// The text of a path node's final segment: the whole text of an `identifier`,
/// or the `name` field of a `scoped_identifier`.
fn final_segment<'a>(path: Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    match path.kind() {
        "identifier" => path.utf8_text(source).ok(),
        "scoped_identifier" => path.child_by_field_name("name")?.utf8_text(source).ok(),
        _ => None,
    }
}

/// True if `name` is PascalCase: starts with an ASCII uppercase letter and
/// contains at least one ASCII lowercase letter. This distinguishes an enum
/// type/variant (`ErrorCode`, `InvalidParams`) from a SCREAMING_SNAKE_CASE
/// const (`MAX_LEN`) and a lowercase function/module name (`value`).
fn is_pascal_case(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
        && name.bytes().any(|b| b.is_ascii_lowercase())
}

/// True if `node`'s nearest enclosing `impl_item` targets a fieldless
/// `enum_item` (by `type_identifier` name) defined in the same file.
fn self_enum_is_fieldless(node: Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "impl_item" {
            return ancestor
                .child_by_field_name("type")
                .filter(|target| target.kind() == "type_identifier")
                .and_then(|target| target.utf8_text(source).ok())
                .and_then(|enum_name| find_enum_item(node, enum_name, source))
                .is_some_and(enum_is_fieldless);
        }
        current = ancestor.parent();
    }
    false
}

/// The first `enum_item` named `name` in the file containing `node`, or `None`.
fn find_enum_item<'a>(node: Node<'a>, name: &str, source: &[u8]) -> Option<Node<'a>> {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        if current.kind() == "enum_item"
            && current
                .child_by_field_name("name")
                .and_then(|name_node| name_node.utf8_text(source).ok())
                == Some(name)
        {
            return Some(current);
        }
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    None
}

/// True if `enum_item` is fieldless — no variant carries a payload. A payload is
/// a `field_declaration_list` (struct variant) or `ordered_field_declaration_list`
/// (tuple variant) child of an `enum_variant`. A discriminant-only variant
/// (`Variant = 1`) carries no such child and stays fieldless.
fn enum_is_fieldless(enum_item: Node) -> bool {
    let Some(body) = enum_item.child_by_field_name("body") else {
        return false;
    };
    let mut variant_cursor = body.walk();
    for variant in body.named_children(&mut variant_cursor) {
        if variant.kind() != "enum_variant" {
            continue;
        }
        let mut field_cursor = variant.walk();
        if variant.named_children(&mut field_cursor).any(|child| {
            matches!(
                child.kind(),
                "field_declaration_list" | "ordered_field_declaration_list"
            )
        }) {
            return false;
        }
    }
    true
}

/// True if `node` is a const-or-path pattern that binds nothing — it pins a
/// match arm to one specific known value rather than capturing it.
///
/// Used on the inner payload of an `Err(...)` `tuple_struct_pattern` to tell the
/// self-documenting lock-free CAS idiom (`Err(Self::REGISTERED) => {}` — "already
/// in this exact state, nothing to do") apart from genuine error-swallowing
/// (`Err(e) => {}`). Two arms qualify:
///
/// - `scoped_identifier` (`Self::REGISTERED`, `Foo::BAR`) — a qualified path is
///   always a const/associated-item reference, never a fresh binding.
/// - `identifier` in SCREAMING_SNAKE_CASE (`REGISTERED`, `MAX_RETRIES`) — Rust
///   convention reserves all-uppercase names for consts. The heuristic requires
///   at least two characters, at least one ASCII uppercase letter, and no ASCII
///   lowercase letter. This rejects a single-uppercase-letter name (`X`) and any
///   mixed-case name (`Frame`, a unit-variant pattern) as not-a-const, and — by
///   definition — a lowercase `identifier` (`e`, `state`, `frame`), which is a
///   FRESH BINDING and must stay flagged.
fn is_const_or_path_pattern(node: Node, source: &[u8]) -> bool {
    match node.kind() {
        "scoped_identifier" => true,
        "identifier" => node.utf8_text(source).is_ok_and(is_screaming_snake),
        _ => false,
    }
}

/// True if `name` follows Rust's SCREAMING_SNAKE_CASE const convention: at least
/// two characters, at least one ASCII uppercase letter, and no ASCII lowercase
/// letter. Interior digits and underscores are allowed alongside the uppercase
/// letters, but a leading underscore is rejected: in pattern position a
/// `_`-prefixed identifier (`_FOO`) is an intentionally-unused binding, not a
/// const reference, so it must not be classified as a const.
fn is_screaming_snake(name: &str) -> bool {
    name.len() >= 2
        && !name.starts_with('_')
        && name.bytes().any(|b| b.is_ascii_uppercase())
        && !name.bytes().any(|b| b.is_ascii_lowercase())
}

/// True if `tuple_struct_pattern` (e.g. `Err(Self::REGISTERED)`) wraps a single
/// payload that is a const-or-path pattern — see [`is_const_or_path_pattern`].
///
/// The payload is the lone named child that is not the constructor path (the
/// `type` field, i.e. the `Err`/`Result::Err` head). A pattern with zero or more
/// than one payload (`Err()`, `Foo(a, b)`) is not a single-value const match and
/// returns false.
pub fn tuple_struct_pattern_binds_const(tuple_struct_pattern: Node, source: &[u8]) -> bool {
    let mut cursor = tuple_struct_pattern.walk();
    let payloads: Vec<Node> = tuple_struct_pattern
        .children(&mut cursor)
        .enumerate()
        .filter(|(i, child)| {
            child.is_named()
                && tuple_struct_pattern.field_name_for_child(*i as u32) != Some("type")
        })
        .map(|(_, child)| child)
        .collect();
    matches!(payloads.as_slice(), [payload] if is_const_or_path_pattern(*payload, source))
}

/// Returns the text between a macro invocation's outer delimiter pair. `text`
/// is the whole invocation (`name!( .. )` / `name![ .. ]` / `name!{ .. }`); we
/// find the first delimiter after `!` and its match.
///
/// tree-sitter-rust models macro arguments as an opaque `token_tree`, so rules
/// that need the individual arguments parse the token-tree text directly. This
/// is the shared entry point for that parsing.
pub(crate) fn macro_body(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let open = bytes.iter().position(|&b| matches!(b, b'(' | b'[' | b'{'))?;
    let close = matching_close(bytes, open)?;
    text.get(open + 1..close)
}

/// Index of the delimiter closing the one opened at `open`, skipping nested
/// delimiters and string/char literal contents.
pub(crate) fn matching_close(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i = skip_string_literal(bytes, i);
                continue;
            }
            b'\'' if is_char_literal(bytes, i) => {
                i = skip_char_literal(bytes, i);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Splits a macro body into its top-level arguments (separated by commas at
/// depth 0 of the body), skipping commas inside nested delimiters and
/// string/char literals. A trailing comma yields no empty final argument.
pub(crate) fn split_top_level_args(body: &str) -> Vec<&str> {
    let bytes = body.as_bytes();
    let mut args = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i = skip_string_literal(bytes, i);
                continue;
            }
            b'\'' if is_char_literal(bytes, i) => {
                i = skip_char_literal(bytes, i);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                args.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let tail = body[start..].trim();
    if !tail.is_empty() {
        args.push(&body[start..]);
    }
    args
}

/// If `arg` is exactly a plain (`"..."`) or raw (`r"..."` / `r#"..."#`) string
/// literal, returns its raw inner content (escapes left intact). Returns `None`
/// when the argument is anything else (a `concat!`, a constant, an expression, a
/// byte string, …).
pub(crate) fn string_literal_content(arg: &str) -> Option<String> {
    let bytes = arg.as_bytes();
    let open = bytes.iter().position(|&b| b == b'"')?;
    // Only a raw-string prefix (`r`, `r#`, `r##`, …) or nothing may precede the
    // opening quote. Anything else means the argument is not a bare string
    // literal.
    let prefix = &arg[..open];
    let is_raw = match prefix {
        "" => false,
        _ if prefix.starts_with('r') && prefix[1..].bytes().all(|b| b == b'#') => true,
        _ => return None,
    };
    let end = skip_string_literal(bytes, open);
    // The literal must span the entire argument.
    if end != bytes.len() {
        return None;
    }
    let hashes = prefix.bytes().filter(|&b| b == b'#').count();
    let inner_start = open + 1;
    let inner_end = end - 1 - if is_raw { hashes } else { 0 };
    arg.get(inner_start..inner_end).map(str::to_owned)
}

/// Advances past a string literal starting at the opening `"` at `start`.
/// Detects raw strings (`r"..."` / `r#"..."#`) by walking back over the `#`s and
/// the `r` prefix: in a raw string backslashes do not escape and the literal
/// ends at `"` followed by the same number of `#`s. In a plain string, `\"` is
/// an escaped quote.
pub(crate) fn skip_string_literal(bytes: &[u8], start: usize) -> usize {
    let mut hashes = 0;
    let mut j = start;
    while j > 0 && bytes[j - 1] == b'#' {
        j -= 1;
        hashes += 1;
    }
    let is_raw = j > 0 && bytes[j - 1] == b'r';
    let hashes = if is_raw { hashes } else { 0 };
    let mut i = start + 1;
    if is_raw {
        while i < bytes.len() {
            if bytes[i] == b'"' && closing_hashes_match(bytes, i + 1, hashes) {
                return i + 1 + hashes;
            }
            i += 1;
        }
    } else {
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'"' => return i + 1,
                _ => i += 1,
            }
        }
    }
    i
}

fn closing_hashes_match(bytes: &[u8], at: usize, hashes: usize) -> bool {
    (0..hashes).all(|k| bytes.get(at + k) == Some(&b'#'))
}

/// Distinguishes a char literal `'c'` / `'\n'` from a lifetime tick. A char
/// literal has a closing `'` within a few bytes; a lifetime (`'a`) does not, so
/// we conservatively require a closing quote.
pub(crate) fn is_char_literal(bytes: &[u8], start: usize) -> bool {
    // `'\X'` or `'X'` — closing quote within 4 bytes accounts for escapes.
    let mut i = start + 1;
    if bytes.get(i) == Some(&b'\\') {
        i += 1;
    }
    i += 1;
    bytes.get(i) == Some(&b'\'')
}

pub(crate) fn skip_char_literal(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    if bytes.get(i) == Some(&b'\\') {
        i += 2;
    } else {
        i += 1;
    }
    // Now at the closing quote.
    i + 1
}

/// True if `enum_item` has at least one variant gated behind a `#[cfg(...)]`
/// (or `#[cfg_attr(...)]`) attribute, making the enum's variant set
/// target-dependent.
///
/// Walks the enum's `enum_variant_list` body; for each `enum_variant`, scans the
/// preceding `attribute_item` siblings (skipping interleaved comments) for an
/// `attribute` whose path child is `cfg` or `cfg_attr`. A variant so gated does
/// not exist on the excluded target, so listing every variant explicitly fails
/// to compile there — a wildcard `_` arm is then the portable, compiler-required
/// way to match such an enum.
///
/// Matching on the `attribute` path child (not raw text) means an unrelated
/// attribute whose name merely ends in `cfg`, or `cfg` appearing in a comment,
/// does not count.
pub fn enum_has_cfg_gated_variant(enum_item: Node, source: &[u8]) -> bool {
    let Some(body) = enum_item.child_by_field_name("body") else {
        return false;
    };
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter(|child| child.kind() == "enum_variant")
        .any(|variant| has_cfg_attribute(variant, source))
}

/// True if `attribute_item`'s `attribute` path child is exactly `cfg` or
/// `cfg_attr`.
fn attribute_is_cfg(attribute_item: Node, source: &[u8]) -> bool {
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
    matches!(path.utf8_text(source), Ok("cfg") | Ok("cfg_attr"))
}

/// True if a local `let` binding named `var`, visible at `node`, is a confirmable
/// `Vec`: it binds `var` to a `Vec`-shaped initializer (`Vec::new()`,
/// `Vec::with_capacity(...)`, `vec![...]`) or carries an explicit `: Vec<...>`
/// type annotation.
///
/// Walks up the enclosing scopes from `node`, considering only `let` declarations
/// that lexically precede `node` within their block. A parameter binding is NOT
/// confirmed here — only an in-scope `let` — so callers that also want to confirm
/// a `Vec`-typed parameter must check that separately.
///
/// `Vec` shares no API with the many other `.push`-/`.iter()`-exposing types
/// (`VecDeque`, crossbeam `Worker`/`Injector`, custom queues), so confirming the
/// binding is `Vec` before suggesting a `Vec`-only rewrite avoids false positives
/// on those types.
pub fn local_let_binds_vec(node: Node, var: &str, source: &[u8]) -> bool {
    let mut child = node;
    while let Some(parent) = child.parent() {
        let mut cursor = parent.walk();
        for sib in parent.children(&mut cursor) {
            if sib.id() == child.id() {
                break;
            }
            if sib.kind() == "let_declaration" && let_binds_vec(sib, var, source) {
                return true;
            }
        }
        child = parent;
    }
    false
}

/// Whether `let_node` declares `var` with a `Vec`-shaped initializer or an
/// explicit `Vec<...>` type annotation.
fn let_binds_vec(let_node: Node, var: &str, source: &[u8]) -> bool {
    let Some(pattern) = let_node.child_by_field_name("pattern") else {
        return false;
    };
    if !let_pattern_binds(pattern, var, source) {
        return false;
    }
    if let Some(ty) = let_node.child_by_field_name("type")
        && ty.utf8_text(source).unwrap_or("").trim_start().starts_with("Vec<")
    {
        return true;
    }
    if let Some(value) = let_node.child_by_field_name("value") {
        let text = value.utf8_text(source).unwrap_or("");
        if text.starts_with("Vec::") || text.starts_with("vec!") {
            return true;
        }
    }
    false
}

/// Whether a local `let` binding for `var`, declared before `node` in an
/// enclosing scope, is an in-memory buffer — a `Vec` or a `String`. Used to
/// recognize the infallible `io::Write`-into-`Vec<u8>` / `fmt::Write`-into-
/// `String` idiom (those impls never return `Err`).
pub fn local_let_binds_buffer(node: Node, var: &str, source: &[u8]) -> bool {
    let mut child = node;
    while let Some(parent) = child.parent() {
        let mut cursor = parent.walk();
        for sib in parent.children(&mut cursor) {
            if sib.id() == child.id() {
                break;
            }
            if sib.kind() == "let_declaration"
                && (let_binds_vec(sib, var, source) || let_binds_string(sib, var, source))
            {
                return true;
            }
        }
        child = parent;
    }
    false
}

/// Whether `let_node` declares `var` with a `String`-shaped initializer or an
/// explicit `String` type annotation.
fn let_binds_string(let_node: Node, var: &str, source: &[u8]) -> bool {
    let Some(pattern) = let_node.child_by_field_name("pattern") else {
        return false;
    };
    if !let_pattern_binds(pattern, var, source) {
        return false;
    }
    if let Some(ty) = let_node.child_by_field_name("type")
        && ty.utf8_text(source).unwrap_or("").trim() == "String"
    {
        return true;
    }
    if let Some(value) = let_node.child_by_field_name("value") {
        let text = value.utf8_text(source).unwrap_or("");
        if text.starts_with("String::") {
            return true;
        }
    }
    false
}

/// Whether a `let` pattern (`x` or `mut x`) binds the name `var`.
fn let_pattern_binds(pattern: Node, var: &str, source: &[u8]) -> bool {
    let name = match pattern.kind() {
        "identifier" => pattern.utf8_text(source).ok(),
        "mut_pattern" => {
            let mut cursor = pattern.walk();
            pattern
                .children(&mut cursor)
                .find(|c| c.kind() == "identifier")
                .and_then(|c| c.utf8_text(source).ok())
        }
        _ => None,
    };
    name == Some(var)
}

/// Whether a local binding for `var` in scope before `node` proves `var` is a
/// scalar integer — an integer-annotated `let` / parameter (`let n: usize`,
/// `fn f(n: u32)`), or an un-annotated `let` whose initializer is a call to a
/// `usize`-returning length / dimension accessor (`let n = arr.dim();`).
///
/// Rust is statically typed, so the binding's annotation, or a length / dimension
/// accessor on its initializer, settles that the value is a count rather than a
/// string or byte sequence. Callers use this to tell a numeric comparison apart
/// from a byte-by-byte one.
pub fn local_binding_is_integer(node: Node, var: &str, source: &[u8]) -> bool {
    if let Some(ty) = find_identifier_type(node, var, source)
        && is_integer_primitive(ty.trim())
    {
        return true;
    }
    local_let_init_is_count(node, var, source)
}

/// True if `name` is an integer primitive — the fixed-width `u8`..`u128` /
/// `i8`..`i128`, plus the platform-width `usize` / `isize`.
fn is_integer_primitive(name: &str) -> bool {
    is_fixed_width_int(name) || matches!(name, "usize" | "isize")
}

/// Whether an un-annotated `let` binding for `var`, declared before `node` in an
/// enclosing scope, is initialized by a length / dimension accessor call.
fn local_let_init_is_count(node: Node, var: &str, source: &[u8]) -> bool {
    let mut child = node;
    while let Some(parent) = child.parent() {
        let mut cursor = parent.walk();
        for sib in parent.children(&mut cursor) {
            if sib.id() == child.id() {
                break;
            }
            if sib.kind() == "let_declaration" && let_init_is_count(sib, var, source) {
                return true;
            }
        }
        child = parent;
    }
    false
}

/// Whether `let_node` binds `var` to a length / dimension accessor call.
fn let_init_is_count(let_node: Node, var: &str, source: &[u8]) -> bool {
    let Some(pattern) = let_node.child_by_field_name("pattern") else {
        return false;
    };
    if !let_pattern_binds(pattern, var, source) {
        return false;
    }
    let_node.child_by_field_name("value").is_some_and(|value| {
        value.kind() == "call_expression" && call_is_count_accessor(value, source)
    })
}

/// Whether `call` is a method call to a length / dimension accessor that returns
/// a `usize` count (`x.len()`, `x.count()`, `arr.dim()`, …). These name the size
/// of a collection or array, never a secret.
fn call_is_count_accessor(call: Node, source: &[u8]) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "field_expression" {
        return false;
    }
    func.child_by_field_name("field")
        .and_then(|f| f.utf8_text(source).ok())
        .is_some_and(|method| {
            matches!(
                method,
                "len" | "count" | "capacity" | "dim" | "dimension" | "ndim" | "nrows" | "ncols"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("grammar should load");
        parser
            .parse(source, None)
            .expect("parser should produce a tree")
    }

    /// Find the first `function_item` node anywhere in the tree.
    fn first_function_item(node: Node) -> Option<Node> {
        if node.kind() == "function_item" {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = first_function_item(child) {
                return Some(found);
            }
        }
        None
    }

    /// Find the first `call_expression` node anywhere in the tree.
    fn first_call_expression(node: Node) -> Option<Node> {
        if node.kind() == "call_expression" {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = first_call_expression(child) {
                return Some(found);
            }
        }
        None
    }

    /// Find the first `type_cast_expression` node anywhere in the tree.
    fn first_type_cast_expression(node: Node) -> Option<Node> {
        if node.kind() == "type_cast_expression" {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = first_type_cast_expression(child) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn cast_in_const_context_distinguishes_const_eval_from_runtime() {
        let cases = [
            // `const` item initializer — the issue's exact shape.
            ("const X: u64 = i32::MIN as u64;", true),
            // `static` item initializer.
            ("static S: u64 = i64::MIN as u64;", true),
            // `const fn` body — fully const-evaluated.
            ("const fn f() -> u32 { let _x = -1i32 as u32; 0 }", true),
            // Array-length type expression `[u8; N as usize]`.
            ("struct A { arr: [u8; LEN as usize] }", true),
            // Array-repeat count expression `[0u8; N as usize]`.
            ("fn g() { let _a = [0u8; LEN as usize]; }", true),
            // `const { … }` block.
            ("fn g() { let _x = const { -1i32 as u32 }; }", true),
            // Array-repeat element inside a const item initializer.
            ("const X: [u8; 4] = [i8::MIN as u8; 4];", true),
            // A plain runtime `let` binding still flags.
            ("fn g(a: i64) { let _x = a as u32; }", false),
            // A normal (non-const) fn body still flags.
            ("fn h(a: i64) -> u32 { a as u32 }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_type_cast_expression(tree.root_node())
                .expect("source should contain a cast");
            assert_eq!(cast_in_const_context(cast, src.as_bytes()), expected, "src: {src}");
        }
    }

    #[test]
    fn is_in_enum_discriminant_distinguishes_discriminant_from_method_body() {
        let cases = [
            // Direct discriminant value — the cast is the variant's `value`.
            ("#[repr(i8)] enum E { Str = b's' as i8 }", true),
            // Nested inside a larger const discriminant expression.
            ("#[repr(i8)] enum E { Str = (b's' as i8) + 1 }", true),
            // A cast in an `impl Enum` method body is a runtime body, not a
            // discriminant.
            (
                "enum E { A } impl E { fn f(&self, x: u32) -> i8 { x as i8 } }",
                false,
            ),
            // A plain function-body cast is never a discriminant.
            ("fn f(x: u32) -> i8 { x as i8 }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_type_cast_expression(tree.root_node())
                .expect("source should contain a cast");
            assert_eq!(is_in_enum_discriminant(cast), expected, "src: {src}");
        }
    }

    #[test]
    fn cast_operand_is_enum_discriminant_distinguishes_fieldless_enum_reads() {
        let cases = [
            // `self as u8` in an `impl` of a fieldless enum reads the discriminant.
            (
                "enum E { A, B } impl E { fn bit(self) -> u32 { 1 << (self as u8) } }",
                true,
            ),
            // `EnumName::Variant as u8` of a fieldless enum.
            ("enum E { A, B, C } fn f() -> u8 { E::A as u8 }", true),
            // Discriminant-only variants are still fieldless.
            (
                "enum E { A = 1, B = 2 } impl E { fn bit(self) -> u8 { self as u8 } }",
                true,
            ),
            // A data-carrying enum: the `as`-cast is not a discriminant read.
            (
                "enum E { A(u32), B } impl E { fn bit(self) -> u8 { self as u8 } }",
                false,
            ),
            // `self` in an `impl` of a struct, not an enum.
            (
                "struct S; impl S { fn bit(self) -> u8 { self as u8 } }",
                false,
            ),
            // A plain numeric operand is never an enum discriminant.
            ("fn f(x: u32) -> u8 { x as u8 }", false),
            // `EnumName::Variant` of a data-carrying enum.
            ("enum E { A(u32), B } fn f() -> u8 { E::B as u8 }", false),
            // An external `<Type>::<Variant>` path (enum not in this file): the
            // shape heuristic reads it as a discriminant access.
            ("fn f() -> u8 { Foo::Bar as u8 }", true),
            // External path with a lowercase final segment is a function/value,
            // not a variant — not a discriminant read.
            ("fn f() -> u8 { module::value as u8 }", false),
            // External path with a SCREAMING_SNAKE final segment is a const.
            ("fn f() -> u8 { limits::MAX_LEN as u8 }", false),
            // The `From<FieldlessEnum> for i32` idiom: the parameter binding `code`
            // is typed as a fieldless in-file enum, so `code as i32` reads the
            // discriminant (issue #6172).
            (
                "enum Code { Ok = 0, Cancelled = 1 } \
                 impl From<Code> for i32 { fn from(code: Code) -> i32 { code as i32 } }",
                true,
            ),
            // A `let`-annotated binding of a fieldless in-file enum, same idiom.
            ("enum E { A, B } fn f() -> u8 { let e: E = E::A; e as u8 }", true),
            // A parameter typed as a DATA-carrying in-file enum: the `as`-cast is
            // not a plain discriminant read.
            ("enum E { A(u32), B } fn f(e: E) -> u8 { e as u8 }", false),
            // A parameter typed as a non-enum in-file type (a struct).
            ("struct W(u32); fn f(w: W) -> u8 { w as u8 }", false),
            // A bare numeric local stays flagged — `i32` names no in-file enum.
            ("fn f() -> u8 { let n: i32 = 0; n as u8 }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_type_cast_expression(tree.root_node())
                .expect("source should contain a cast");
            assert_eq!(
                cast_operand_is_enum_discriminant(cast, src.as_bytes()),
                expected,
                "src: {src}"
            );
        }
    }

    #[test]
    fn cast_operand_literal_value_parses_every_literal_form() {
        let cases = [
            // Decimal, hex, octal, binary — all the same value 65.
            ("fn f() { let _ = 65 as i8; }", Some(65)),
            ("fn f() { let _ = 0x41 as i8; }", Some(65)),
            ("fn f() { let _ = 0o101 as i8; }", Some(65)),
            ("fn f() { let _ = 0b1000001 as i8; }", Some(65)),
            // Digit separators and a type suffix.
            ("fn f() { let _ = 1_000 as u16; }", Some(1000)),
            ("fn f() { let _ = 65u8 as i8; }", Some(65)),
            ("fn f() { let _ = 0xFFi32 as i32; }", Some(255)),
            // Leading unary minus.
            ("fn f() { let _ = -5 as i8; }", Some(-5)),
            ("fn f() { let _ = -128 as i8; }", Some(-128)),
            // Byte literals, including escapes.
            ("fn f() { let _ = b'A' as i8; }", Some(65)),
            ("fn f() { let _ = b' ' as i8; }", Some(32)),
            ("fn f() { let _ = b'\\n' as i8; }", Some(10)),
            ("fn f() { let _ = b'\\x7f' as i8; }", Some(127)),
            ("fn f() { let _ = b'\\\\' as i8; }", Some(92)),
            // Non-literal operands have no statically known value.
            ("fn f(x: i32) { let _ = x as i8; }", None),
            ("fn f(x: u32) { let _ = (x >> 8) as u8; }", None),
            // Float and char literals are deliberately excluded.
            ("fn f() { let _ = 1.0 as f32; }", None),
            ("fn f() { let _ = 'A' as i32; }", None),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_type_cast_expression(tree.root_node())
                .expect("source should contain a cast");
            assert_eq!(
                cast_operand_literal_value(cast, src.as_bytes()),
                expected,
                "src: {src}"
            );
        }
    }

    #[test]
    fn cast_operand_is_repr_enum_field_resolves_struct_field_repr_enums() {
        let cases = [
            // `self.field as u8`: field type is an in-file `#[repr(u8)]` enum.
            (
                "#[repr(u8)] enum E { A, B } struct S { f: E } \
                 impl S { fn ser(&self) -> u8 { self.f as u8 } }",
                true,
            ),
            // `<binding>.field as u8`: annotated parameter, repr(u8) field enum.
            (
                "#[repr(u8)] enum E { A } struct S { f: E } \
                 fn g(s: &S) -> u8 { s.f as u8 }",
                true,
            ),
            // repr(u8) field cast to a wider u16 is still lossless.
            (
                "#[repr(u8)] enum E { A } struct S { f: E } \
                 fn g(s: &S) -> u16 { s.f as u16 }",
                true,
            ),
            // repr(u16) field does not fit u8 — not exempt.
            (
                "#[repr(u16)] enum E { A } struct S { f: E } \
                 fn g(s: &S) -> u8 { s.f as u8 }",
                false,
            ),
            // No `#[repr(intN)]`: discriminant width is unspecified — not exempt.
            (
                "enum E { A } struct S { f: E } fn g(s: &S) -> u8 { s.f as u8 }",
                false,
            ),
            // `#[repr(C)]` is not an integer repr — not exempt.
            (
                "#[repr(C)] enum E { A } struct S { f: E } \
                 fn g(s: &S) -> u8 { s.f as u8 }",
                false,
            ),
            // The field is a plain wider integer, not a repr-enum — not exempt.
            (
                "struct S { count: u16 } fn g(s: &S) -> u8 { s.count as u8 }",
                false,
            ),
            // Unknown receiver type (inferred binding) — cannot resolve, not exempt.
            (
                "#[repr(u8)] enum E { A } struct S { f: E } \
                 fn g() -> u8 { let s = make(); s.f as u8 }",
                false,
            ),
            // repr(u8) field to a signed i8: an i8 cannot hold a u8 discriminant
            // of 128..=255, so it does not fit — not exempt.
            (
                "#[repr(u8)] enum E { A } struct S { f: E } \
                 fn g(s: &S) -> i8 { s.f as i8 }",
                false,
            ),
            // repr(u8) field to a wider signed i16 fits — exempt.
            (
                "#[repr(u8)] enum E { A } struct S { f: E } \
                 fn g(s: &S) -> i16 { s.f as i16 }",
                true,
            ),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_type_cast_expression(tree.root_node())
                .expect("source should contain a cast");
            let target = cast
                .child_by_field_name("type")
                .and_then(|t| t.utf8_text(src.as_bytes()).ok())
                .map(str::trim)
                .expect("cast should have a target type");
            assert_eq!(
                cast_operand_is_repr_enum_field(cast, src.as_bytes(), target),
                expected,
                "src: {src}"
            );
        }
    }

    /// Find the `.unwrap()` / `.expect(...)` `call_expression` (the innermost
    /// such call) anywhere in the tree.
    fn first_unwrap_call<'a>(node: Node<'a>, source: &[u8]) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = first_unwrap_call(child, source) {
                return Some(found);
            }
        }
        if node.kind() == "call_expression"
            && let Some(function) = node.child_by_field_name("function")
            && function.kind() == "field_expression"
            && let Some(field) = function.child_by_field_name("field")
            && let Ok(text) = field.utf8_text(source)
            && (text == "unwrap" || text == "expect")
        {
            return Some(node);
        }
        None
    }

    #[test]
    fn is_in_const_initializer_distinguishes_initializer_from_const_fn_body() {
        let cases = [
            // Const item initializer — the canonical `NonZeroU32::new(_).unwrap()`.
            (
                "impl W { pub const ONE: W = W(NonZeroU32::new(1).unwrap()); }",
                true,
            ),
            // Static item initializer.
            ("static S: u32 = foo().unwrap();", true),
            // A `const fn` body is a runtime body that can return `Result`.
            ("const fn f(x: Option<u32>) -> u32 { x.unwrap() }", false),
            // A plain function-body unwrap is never a const initializer.
            ("fn f(x: Option<u32>) -> u32 { x.unwrap() }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let call = first_unwrap_call(tree.root_node(), src.as_bytes())
                .expect("source should contain an unwrap/expect call");
            assert_eq!(is_in_const_initializer(call), expected, "src: {src}");
        }
    }

    #[test]
    fn tuple_struct_pattern_binds_const_distinguishes_const_from_binding() {
        let cases = [
            // scoped_identifier payload — always a path/const, never a binding.
            ("fn f(r: R) { match r { Err(Self::REGISTERED) => {} } }", true),
            ("fn f(r: R) { match r { Err(Foo::BAR) => {} } }", true),
            // A qualified `Result::Err` head must not be mistaken for the payload.
            ("fn f(r: R) { match r { Result::Err(Self::REGISTERED) => {} } }", true),
            // SCREAMING_SNAKE identifier — a const by convention.
            ("fn f(r: R) { match r { Err(MAX_RETRIES) => {} } }", true),
            ("fn f(r: R) { match r { Err(REGISTERED) => {} } }", true),
            // Fresh lowercase bindings — must NOT be exempted.
            ("fn f(r: R) { match r { Err(e) => {} } }", false),
            ("fn f(r: R) { match r { Err(frame) => {} } }", false),
            ("fn f(r: R) { match r { Err(_state) => {} } }", false),
            // A leading-underscore SCREAMING name is an intentionally-unused
            // binding in pattern position, not a const reference.
            ("fn f(r: R) { match r { Err(_FOO) => {} } }", false),
            // Wildcard is the `_` token, not a binding identifier.
            ("fn f(r: R) { match r { Err(_) => {} } }", false),
            // Mixed-case identifier (a unit-variant pattern) is not a const.
            ("fn f(r: R) { match r { Err(Frame) => {} } }", false),
            // Single uppercase letter is rejected by the boundary rule.
            ("fn f(r: R) { match r { Err(X) => {} } }", false),
            // A multi-arg tuple struct is not a single-value const match.
            ("fn f(r: R) { match r { Err(A, B) => {} } }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let pat = first_of_kind(tree.root_node(), "tuple_struct_pattern")
                .expect("snippet should contain a tuple_struct_pattern");
            assert_eq!(
                tuple_struct_pattern_binds_const(pat, src.as_bytes()),
                expected,
                "tuple_struct_pattern_binds_const mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn canonical_is_pub_excludes_pub_crate_and_pub_super() {
        let cases = [
            ("pub fn f() {}", true),
            ("pub(crate) fn f() {}", false),
            ("pub(super) fn f() {}", false),
            ("pub(in crate::a) fn f() {}", false),
            ("fn f() {}", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let func = first_function_item(tree.root_node())
                .expect("snippet should contain a function_item");
            assert_eq!(
                is_pub(func, src.as_bytes()),
                expected,
                "is_pub mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn is_inside_non_public_module_walks_enclosing_modules() {
        let cases = [
            // A non-public enclosing module confines the inner item.
            ("pub(crate) mod m { pub use foo::*; }", true),
            ("pub(super) mod m { pub use foo::*; }", true),
            ("pub(in crate::a) mod m { pub use foo::*; }", true),
            ("mod m { pub use foo::*; }", true),
            // A bare-`pub` enclosing module leaves visibility public.
            ("pub mod m { pub use foo::*; }", false),
            // Nested: a private module anywhere in the chain confines it,
            // even when the innermost module is bare-`pub`.
            ("pub(crate) mod outer { pub mod inner { pub use foo::*; } }", true),
            // All-public chain: nothing confines the item.
            ("pub mod outer { pub mod inner { pub use foo::*; } }", false),
            // File scope: no enclosing module at all.
            ("pub use foo::*;", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let use_decl = first_of_kind(tree.root_node(), "use_declaration")
                .expect("snippet should contain a use_declaration");
            assert_eq!(
                is_inside_non_public_module(use_decl, src.as_bytes()),
                expected,
                "is_inside_non_public_module mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn is_effectively_pub_combines_own_and_enclosing_visibility() {
        let cases = [
            // Bare-`pub` at file scope: effectively public.
            ("pub fn f() {}", true),
            // Bare-`pub` inside a bare-`pub mod`: still effectively public.
            ("pub mod m { pub fn f() {} }", true),
            // Non-public own modifier: not public regardless of enclosing module.
            ("pub(crate) fn f() {}", false),
            ("fn f() {}", false),
            // Bare-`pub` confined to a non-public module: not effectively public.
            ("mod imp { pub fn f() {} }", false),
            ("pub(crate) mod m { pub fn f() {} }", false),
            // Nested: a private module anywhere in the chain confines it.
            ("pub(crate) mod outer { pub mod inner { pub fn f() {} } }", false),
        ];
        // A path with no resolvable parent module file on disk, so the
        // cross-file check is a no-op and only the in-file visibility logic
        // under test applies.
        let path = Path::new("/nonexistent_comply_test/src/t.rs");
        for (src, expected) in cases {
            let tree = parse(src);
            let func = first_of_kind(tree.root_node(), "function_item")
                .expect("snippet should contain a function_item");
            assert_eq!(
                is_effectively_pub(func, src.as_bytes(), path),
                expected,
                "is_effectively_pub mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn is_in_trait_impl_distinguishes_trait_from_inherent() {
        let trait_impl = "struct T; impl Tr for T { fn m(&self) {} }";
        let inherent_impl = "struct T; impl T { fn m(&self) {} }";
        let free_fn = "fn m() {}";

        let cases = [
            (trait_impl, true),
            (inherent_impl, false),
            (free_fn, false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let func = first_function_item(tree.root_node())
                .expect("snippet should contain a function_item");
            assert_eq!(
                is_in_trait_impl(func),
                expected,
                "is_in_trait_impl mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn is_in_trait_definition_distinguishes_trait_def_from_free_and_inherent() {
        // Anchor on the `Result<…>` return type (`generic_type`) — the same
        // node the `rust-string-as-error` rule fires on.
        let trait_def = "trait T { fn f() -> Result<(), String>; }";
        let free_fn = "fn f() -> Result<(), String> { Ok(()) }";
        let inherent_impl = "struct S; impl S { fn f() -> Result<(), String> { Ok(()) } }";

        let cases = [
            (trait_def, true),
            (free_fn, false),
            (inherent_impl, false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let ret = first_of_kind(tree.root_node(), "generic_type")
                .expect("snippet should contain a generic_type");
            assert_eq!(
                is_in_trait_definition(ret),
                expected,
                "is_in_trait_definition mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn result_ok_type_returns_first_positional_arg() {
        // `Result<(), E>` → the ok type is the `unit_type` first arg.
        let unit_ok = "fn f() -> Result<(), String> { Ok(()) }";
        let tree = parse(unit_ok);
        let result = first_of_kind(tree.root_node(), "generic_type")
            .expect("snippet should contain a generic_type");
        let ok = result_ok_type(result, unit_ok.as_bytes())
            .expect("Result<(), E> should expose an ok type");
        assert_eq!(ok.kind(), "unit_type");

        // `Result<i32, ()>` → the ok type is the value, not the unit error.
        let value_ok = "fn f() -> Result<i32, ()> { Ok(0) }";
        let tree = parse(value_ok);
        let result = first_of_kind(tree.root_node(), "generic_type")
            .expect("snippet should contain a generic_type");
        let ok = result_ok_type(result, value_ok.as_bytes())
            .expect("Result<i32, ()> should expose an ok type");
        assert_ne!(ok.kind(), "unit_type");
        assert_eq!(ok.utf8_text(value_ok.as_bytes()).unwrap(), "i32");
    }

    #[test]
    fn has_doc_hidden_matches_doc_hidden_past_cfg_and_comments() {
        let cases = [
            ("#[doc(hidden)]\npub use x::*;", true),
            // doc(hidden) sits beside a cfg — must traverse past it.
            ("#[cfg(feature = \"derive\")]\n#[doc(hidden)]\npub use x::*;", true),
            // interleaved comment between attribute and item.
            ("#[doc(hidden)]\n// note\npub use x::*;", true),
            // bare, no doc(hidden).
            ("pub use x::*;", false),
            // cfg only — not doc(hidden).
            ("#[cfg(feature = \"derive\")]\npub use x::*;", false),
            // doc string reading "hidden" is not doc(hidden).
            ("#[doc = \"hidden\"]\npub use x::*;", false),
            // a different doc argument.
            ("#[doc(inline)]\npub use x::*;", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let item = first_of_kind(tree.root_node(), "use_declaration")
                .expect("snippet should contain a use_declaration");
            assert_eq!(
                has_doc_hidden(item, src.as_bytes()),
                expected,
                "has_doc_hidden mismatch for `{src}`"
            );
        }
    }

    /// Find the first node of `kind` anywhere in the tree.
    fn first_of_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = first_of_kind(child, kind) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn is_in_test_context_recognizes_compound_cfg() {
        // A `macro_invocation` (e.g. `eprintln!`) is what the affected rules
        // anchor on; reproduce the jiff FP from #1324 with one.
        let test_cases = [
            ("#[cfg(test)]\nmod m { fn f() { eprintln!(\"x\"); } }", true),
            ("#[cfg(all(test, not(loom)))]\nmod m { fn f() { eprintln!(\"x\"); } }", true),
            ("#[cfg(any(test, fuzzing))]\nmod m { fn f() { eprintln!(\"x\"); } }", true),
            (
                "#[cfg(all(test, feature = \"std\", feature = \"logging\"))]\nimpl T { fn f(&self) { eprintln!(\"x\"); } }",
                true,
            ),
            (
                "#[cfg(all(feature = \"std\", any(test, fuzzing)))]\nfn f() { eprintln!(\"x\"); }",
                true,
            ),
            // Negative space: `not(test)` is production-only, not test context.
            ("#[cfg(not(test))]\nmod m { fn f() { eprintln!(\"x\"); } }", false),
            ("#[cfg(all(not(test), unix))]\nfn f() { eprintln!(\"x\"); }", false),
            ("#[cfg(feature = \"std\")]\nfn f() { eprintln!(\"x\"); }", false),
            ("fn f() { eprintln!(\"x\"); }", false),
        ];
        for (src, expected) in test_cases {
            let tree = parse(src);
            let node = first_of_kind(tree.root_node(), "macro_invocation")
                .expect("snippet should contain a macro_invocation");
            assert_eq!(
                is_in_test_context(node, src.as_bytes()),
                expected,
                "is_in_test_context mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn has_test_attribute_recognizes_known_and_compound_forms() {
        let test_cases = [
            ("#[test]\nfn f() {}", true),
            ("#[cfg(test)]\nmod m {}", true),
            ("#[cfg_attr(test, derive(Debug))]\nstruct S;", true),
            ("#[tokio::test]\nasync fn f() {}", true),
            ("#[tokio::test(flavor = \"multi_thread\")]\nasync fn f() {}", true),
            ("#[cfg(all(test, not(loom)))]\nmod m {}", true),
            ("#[cfg(any(test, fuzzing))]\nmod m {}", true),
            ("#[cfg(all(test, feature = \"std\"))]\nmod m {}", true),
            // Doc comments may interleave the attribute and the item in any
            // order; they must not terminate the attribute scan (issue #4496).
            ("#[cfg(test)]\n/// Tests.\nmod m {}", true),
            ("#[cfg(test)]\n/// a\n/// b\nmod m {}", true),
            ("#[cfg(test)]\n/** doc */\nmod m {}", true),
            ("#[test]\n/// doc\nfn f() {}", true),
            // Negative space.
            ("/// docs\nmod m {}", false),
            ("#[cfg(not(test))]\nmod m {}", false),
            ("#[cfg(feature = \"std\")]\nfn f() {}", false),
            ("#[derive(Debug)]\nstruct S;", false),
            ("fn f() {}", false),
        ];
        for (src, expected) in test_cases {
            let tree = parse(src);
            // The decorated item is the last named child of the source file;
            // attributes precede it as `attribute_item` siblings.
            let root = tree.root_node();
            let item = root
                .named_child(root.named_child_count().saturating_sub(1))
                .expect("snippet should contain an item");
            assert_eq!(
                has_test_attribute(item, src.as_bytes()),
                expected,
                "has_test_attribute mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn is_suppressed_by_clippy_allow_honors_matching_allow_and_ignores_others() {
        let test_cases = [
            // Function-level `#[allow(clippy::result_unit_err)]` suppresses.
            (
                "#[allow(clippy::result_unit_err)]\nfn f() -> Result<(), ()> { Ok(()) }",
                true,
            ),
            // The `#[expect(...)]` form suppresses too.
            (
                "#[expect(clippy::result_unit_err)]\nfn f() -> Result<(), ()> { Ok(()) }",
                true,
            ),
            // Crate-root inner attribute suppresses.
            (
                "#![allow(clippy::result_unit_err)]\nfn f() -> Result<(), ()> { Ok(()) }",
                true,
            ),
            // An unrelated allow (not the named lint) does not suppress.
            (
                "#[allow(dead_code)]\nfn f() -> Result<(), ()> { Ok(()) }",
                false,
            ),
            // No attribute at all.
            ("fn f() -> Result<(), ()> { Ok(()) }", false),
        ];
        for (src, expected) in test_cases {
            let tree = parse(src);
            let node = first_of_kind(tree.root_node(), "generic_type")
                .expect("snippet should contain a generic_type");
            assert_eq!(
                is_suppressed_by_clippy_allow(node, &["result_unit_err"], src.as_bytes()),
                expected,
                "is_suppressed_by_clippy_allow mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn is_suppressed_by_clippy_allow_honors_struct_and_field_scopes() {
        let test_cases = [
            // Field-level `#[allow]` over the field's own type suppresses.
            (
                "struct S { #[allow(clippy::mutex_atomic)] x: Mutex<bool> }",
                true,
            ),
            // Struct-level `#[allow]` covers a type in one of its fields.
            (
                "#[allow(clippy::mutex_atomic)]\nstruct S { x: Mutex<bool> }",
                true,
            ),
            // The `#[expect(...)]` form on the field suppresses too.
            (
                "struct S { #[expect(clippy::mutex_atomic)] x: Mutex<bool> }",
                true,
            ),
            // A different clippy lint on the field does not suppress.
            (
                "struct S { #[allow(clippy::other_lint)] x: Mutex<bool> }",
                false,
            ),
            // No attribute at all.
            ("struct S { x: Mutex<bool> }", false),
        ];
        for (src, expected) in test_cases {
            let tree = parse(src);
            let node = first_of_kind(tree.root_node(), "generic_type")
                .expect("snippet should contain a generic_type");
            assert_eq!(
                is_suppressed_by_clippy_allow(node, &["mutex_atomic"], src.as_bytes()),
                expected,
                "is_suppressed_by_clippy_allow mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn is_suppressed_by_clippy_allow_honors_statement_scope() {
        let test_cases = [
            // Statement-level `#[allow]` on a `{ … }` block covers a node inside.
            (
                "fn f() { #[allow(clippy::disallowed_macros)] { eprintln!(\"x\"); } }",
                true,
            ),
            // The `#[expect(...)]` form on the block suppresses too.
            (
                "fn f() { #[expect(clippy::disallowed_macros)] { eprintln!(\"x\"); } }",
                true,
            ),
            // A different clippy lint on the block does not suppress.
            (
                "fn f() { #[allow(clippy::print_stderr)] { eprintln!(\"x\"); } }",
                false,
            ),
            // No attribute on the enclosing block.
            ("fn f() { { eprintln!(\"x\"); } }", false),
        ];
        for (src, expected) in test_cases {
            let tree = parse(src);
            let node = first_of_kind(tree.root_node(), "macro_invocation")
                .expect("snippet should contain a macro_invocation");
            assert_eq!(
                is_suppressed_by_clippy_allow(node, &["disallowed_macros"], src.as_bytes()),
                expected,
                "is_suppressed_by_clippy_allow mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn cfg_predicate_scan_does_not_panic_on_multibyte_attribute_literals() {
        // A non-ASCII attribute literal must not panic the byte-cursor scan,
        // even when the cursor walks through the interior of a multi-byte char.
        assert!(!cfg_predicate_activates_test(
            "#[test_case(\"中-broken\"; \"case\")]"
        ));
        assert!(!attr_marks_test("#[doc = \"中\"]"));
    }

    #[test]
    fn cfg_predicate_detection_holds_with_multibyte_chars_present() {
        let cases = [
            ("#[cfg(test)]", true),
            ("#[cfg(not(test))]", false),
            // A multi-byte literal inside the group must neither break detection
            // nor panic the scan.
            ("#[cfg(all(feature = \"中\", test))]", true),
        ];
        for (text, expected) in cases {
            assert_eq!(
                cfg_predicate_activates_test(text),
                expected,
                "cfg_predicate_activates_test mismatch for `{text}`"
            );
        }
    }

    #[test]
    fn is_in_test_context_handles_multibyte_test_case_attribute() {
        // Issue #3732 repro: a `#[test_case]` with a non-ASCII literal inside a
        // `#[cfg(test)] mod`. Must not panic, and the `#[cfg(test)]` mod must
        // still be recognized as test context.
        let src = "#[cfg(test)]\nmod tests { #[test_case(\"中-broken\"; \"case\")]\nfn t() { let x: Option<u32> = Some(1); x.unwrap(); } }";
        let tree = parse(src);
        let node = first_of_kind(tree.root_node(), "call_expression")
            .expect("snippet should contain a call_expression");
        assert!(
            is_in_test_context(node, src.as_bytes()),
            "the #[cfg(test)] mod should be recognized as test context"
        );
    }

    #[test]
    fn is_under_tests_dir_matches_segments_and_file_names_exactly() {
        use std::path::Path;
        let cases = [
            // Existing `tests/` behavior, any depth.
            ("tests/helpers.rs", true),
            ("crates/foo/tests/it.rs", true),
            // Snake_case test-infrastructure segments (delimited token).
            ("crates/foo/src/types/property_tests/gen.rs", true),
            ("crates/foo/src/test_utils/db.rs", true),
            ("crates/foo/src/test_helpers/mod.rs", true),
            ("crates/foo/src/property_tests_old/gen.rs", true),
            // Kebab-case test-infrastructure segments (delimited token).
            ("integration-tests/src/env.rs", true),
            ("integration-tests/foo/src/lib.rs", true),
            ("test-helpers/src/lib.rs", true),
            ("crates/foo/e2e-tests/run.rs", true),
            ("crates/foo/end-to-end-tests/run.rs", true),
            ("crates/foo/test-utils/db.rs", true),
            // Prefix-only segments (`test` not a delimited token) still match.
            ("crates/foo/src/testing/mod.rs", true),
            ("crates/foo/src/testutil/mod.rs", true),
            // New exact file names (cross-crate test helpers, no #[cfg(test)]).
            ("crates/foo/src/testing.rs", true),
            ("crates/foo/src/test_utils.rs", true),
            ("crates/foo/src/test_helpers.rs", true),
            ("crates/searcher/src/testutil.rs", true),
            // Inline-test-module convention: `mod tests;` -> sibling `tests.rs`
            // (and the singular `test.rs`) under `src/**/`.
            ("crates/foo/src/payload_storage/tests.rs", true),
            ("crates/foo/src/index/numeric_index/tests.rs", true),
            ("crates/foo/src/parser/test.rs", true),
            // Negative space: non-exact segments / file names are production.
            ("crates/foo/src/lib.rs", false),
            ("crates/foo/src/my_testing.rs", false),
            ("crates/foo/src/testingground/k.rs", false),
            // `test`/`tests` as a non-delimited substring of a longer file stem
            // is production code, not the inline-test-module file.
            ("crates/foo/src/latest.rs", false),
            ("crates/foo/src/contest.rs", false),
            ("crates/foo/src/greatest.rs", false),
            // `test` as a non-delimited substring is NOT a test dir.
            ("crates/foo/src/latest/v.rs", false),
            ("crates/foo/src/greatest/v.rs", false),
            ("crates/foo/src/contest/v.rs", false),
            ("crates/foo/src/attestation/v.rs", false),
        ];
        for (path, expected) in cases {
            assert_eq!(
                is_under_tests_dir(Path::new(path)),
                expected,
                "is_under_tests_dir mismatch for `{path}`"
            );
        }
    }

    #[test]
    fn cast_operand_is_collection_size_matches_size_methods_only() {
        let cases = [
            ("fn f(d: D) -> u32 { d.hunks.len() as u32 }", true),
            ("fn f(&self) -> u32 { self.diff.hunks.len() as u32 }", true),
            ("fn f(v: V) -> u16 { v.iter().count() as u16 }", true),
            ("fn f(v: V) -> u32 { v.capacity() as u32 }", true),
            // Same-named methods with arguments are not the size accessors.
            ("fn f(v: V) -> u32 { v.count(2) as u32 }", false),
            // A non-size method is unbounded — must not be exempted.
            ("fn f(v: V) -> u8 { v.parse_count() as u8 }", false),
            // A bare identifier operand has no call shape.
            ("fn f(n: usize) -> u32 { n as u32 }", false),
            // A free function `len(x)` is not a field-method call.
            ("fn f() -> u32 { len(x) as u32 }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_of_kind(tree.root_node(), "type_cast_expression")
                .expect("snippet should contain a type_cast_expression");
            assert_eq!(
                cast_operand_is_collection_size(cast, src.as_bytes()),
                expected,
                "cast_operand_is_collection_size mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn cast_operand_bit_count_max_matches_bit_count_methods_only() {
        let cases = [
            ("fn f(x: u32) -> u8 { x.leading_zeros() as u8 }", Some(128)),
            ("fn f(x: u64) -> u16 { x.trailing_zeros() as u16 }", Some(128)),
            ("fn f(x: u128) -> u8 { x.count_ones() as u8 }", Some(128)),
            ("fn f(x: u64) -> u8 { x.count_zeros() as u8 }", Some(128)),
            ("fn f(x: u32) -> u8 { x.leading_ones() as u8 }", Some(128)),
            ("fn f(x: u32) -> u8 { x.trailing_ones() as u8 }", Some(128)),
            // A parenthesized receiver is fine — the operand is still the call.
            ("fn f(x: u64) -> i16 { (x ^ x).leading_zeros() as i16 }", Some(128)),
            // A non-bit-count method is unbounded.
            ("fn f(v: V) -> u8 { v.some_other_method() as u8 }", None),
            // A same-named method taking arguments is not the bit-count accessor.
            ("fn f(x: u32) -> u8 { x.count_ones(2) as u8 }", None),
            // Arithmetic on the result is no longer a bare call operand.
            ("fn f(x: u64, o: u32) -> u8 { (x.leading_zeros() + o) as u8 }", None),
            // A bare identifier operand has no call shape.
            ("fn f(n: usize) -> u32 { n as u32 }", None),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_of_kind(tree.root_node(), "type_cast_expression")
                .expect("snippet should contain a type_cast_expression");
            assert_eq!(
                cast_operand_bit_count_max(cast, src.as_bytes()),
                expected,
                "cast_operand_bit_count_max mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn cast_operand_bit_width_reads_literal_bit_count() {
        let cases = [
            // Canonical bit-reader idioms with `?`.
            ("fn f(bs: B) -> u8 { bs.read_bits_leq32(8)? as u8 }", Some(8)),
            ("fn f(bs: B) -> u16 { bs.read_bits_leq32(16)? as u16 }", Some(16)),
            ("fn f(r: R) -> u8 { r.get_bits(2)? as u8 }", Some(2)),
            ("fn f(r: R) -> u8 { r.peek_bits(1)? as u8 }", Some(1)),
            // No `?`.
            ("fn f(r: R) -> u8 { r.read_bits(4) as u8 }", Some(4)),
            // Parenthesized operand is transparent.
            ("fn f(r: R) -> u8 { (r.read_bits(5)) as u8 }", Some(5)),
            // A larger decimal count.
            ("fn f(r: R) -> u16 { r.read_bits(12) as u16 }", Some(12)),
            // A decimal type suffix is stripped.
            ("fn f(r: R) -> u8 { r.read_bits(8u32) as u8 }", Some(8)),
            // A radix-prefixed count is not parsed as decimal — not bounded.
            ("fn f(r: R) -> u8 { r.read_bits(0xFF) as u8 }", None),
            // A method name *containing* `bits` matches (case-insensitive).
            ("fn f(r: R) -> u8 { r.GetBitsLE(3)? as u8 }", Some(3)),
            // A method whose name has no `bits` is a byte/other read — not matched.
            ("fn f(r: R) -> u8 { r.read(1)? as u8 }", None),
            ("fn f(r: R) -> u8 { r.read_u8()? as u8 }", None),
            // Non-literal count is not statically bounded.
            ("fn f(r: R, n: u32) -> u8 { r.read_bits(n)? as u8 }", None),
            // Zero or multiple arguments do not match the single-count shape.
            ("fn f(r: R) -> u8 { r.read_bits() as u8 }", None),
            ("fn f(r: R) -> u8 { r.read_bits(1, 2) as u8 }", None),
            // A free function `read_bits(8)` is not a method call.
            ("fn f() -> u8 { read_bits(8) as u8 }", None),
            // A bare identifier operand has no call shape.
            ("fn f(n: u32) -> u8 { n as u8 }", None),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_of_kind(tree.root_node(), "type_cast_expression")
                .expect("snippet should contain a type_cast_expression");
            assert_eq!(
                cast_operand_bit_width(cast, src.as_bytes()),
                expected,
                "cast_operand_bit_width mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn cast_operand_is_bool_recognizes_bool_producing_operands() {
        let cases = [
            // Boolean literal.
            ("fn f() -> u8 { true as u8 }", true),
            // Comparison operators always yield bool.
            ("fn f() -> u8 { (3 > 2) as u8 }", true),
            ("fn f(a: i32, b: i32) -> u8 { (a == b) as u8 }", true),
            // Logical operators yield bool.
            ("fn f(a: bool, b: bool) -> u8 { (a && b) as u8 }", true),
            // `!` on a bool operand yields bool.
            ("fn f(b: bool) -> u8 { (!b) as u8 }", true),
            ("fn f() -> u8 { !true as u8 }", true),
            // `!` on an integer is bitwise NOT and stays integer — NOT bool.
            ("fn f(x: u32) -> u8 { !x as u8 }", false),
            ("fn f() -> u8 { !5 as u8 }", false),
            // Convention-named bool methods.
            ("fn f(o: Option<i32>) -> u8 { o.is_some() as u8 }", true),
            ("fn f(m: M) -> u8 { m.has_key() as u8 }", true),
            ("fn f(s: &str) -> u8 { s.contains(\"x\") as u8 }", true),
            ("fn f(s: &str) -> u8 { s.starts_with(\"x\") as u8 }", true),
            ("fn f(s: &str) -> u8 { s.ends_with(\"x\") as u8 }", true),
            // Method call resolved to a same-file `-> bool` definition (#5886).
            (
                "fn get_random_bit(&self) -> bool { true } \
                 fn f(&self) -> u8 { self.get_random_bit() as u8 }",
                true,
            ),
            // Free function call resolved to a same-file `-> bool` definition.
            ("fn g() -> bool { true } fn f() -> u8 { g() as u8 }", true),
            // Same-file callee returning a non-bool numeric — not bool.
            (
                "fn tally(&self) -> u32 { 0 } fn f(&self) -> u8 { self.tally() as u8 }",
                false,
            ),
            // Callee not defined in the file — unresolved, stays flagged.
            ("fn f(s: S) -> u8 { s.unknown() as u8 }", false),
            // Identifier whose binding is annotated bool.
            ("fn f(b: bool) -> u8 { b as u8 }", true),
            // #6090: an inferred local bound to a bool comparison initializer.
            ("fn f(lo: f64, x: f64) -> u8 { let e = lo <= x; e as u8 }", true),
            // An inferred local bound to a logical expression is bool.
            ("fn f(a: bool, b: bool) -> u8 { let e = a && b; e as u8 }", true),
            // An inferred local bound to an arithmetic expression is NOT bool.
            ("fn f(a: u32, b: u32) -> u8 { let n = a + b; n as u8 }", false),
            // The nearest shadow decides: a later non-bool `n` masks an earlier
            // bool `n`, so the cast operand is NOT bool.
            (
                "fn f(a: u32, b: u32) -> u8 { let n = a < b; let n = a + b; n as u8 }",
                false,
            ),
            // A binding in a sibling inner block does not enclose the cast, so it
            // must not be picked up — the outer `n` (a param, non-bool) governs.
            (
                "fn f(n: u32) -> u8 { { let n = n < 1; let _ = n; } n as u8 }",
                false,
            ),
            // A self-referential `let x = x;` must not recurse forever; the
            // shadowed outer `x` is a param (non-bool), so the cast is not bool.
            ("fn f(x: u32) -> u8 { let x = x; x as u8 }", false),
            // A plain integer cast is not a bool operand.
            ("fn f(x: u32) -> u8 { x as u8 }", false),
            // `.len()` returns usize, not bool.
            ("fn f(v: V) -> u8 { v.len() as u8 }", false),
            // An arbitrary method (not in the convention) is not bool.
            ("fn f(v: V) -> u8 { v.count_things() as u8 }", false),
            // Arithmetic binary op is not a comparison/logical op.
            ("fn f(a: i32, b: i32) -> u8 { (a + b) as u8 }", false),
            // A non-bool identifier is not a bool operand.
            ("fn f(x: u32) -> u8 { x as u8 }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_of_kind(tree.root_node(), "type_cast_expression")
                .expect("snippet should contain a type_cast_expression");
            assert_eq!(
                cast_operand_is_bool(cast, src.as_bytes()),
                expected,
                "cast_operand_is_bool mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn cast_operand_is_char_recognizes_char_producing_operands() {
        let cases = [
            // Char literal.
            ("fn f() -> u32 { 'a' as u32 }", true),
            // Identifier annotated char.
            ("fn f(c: char) -> u32 { c as u32 }", true),
            // `chars()` for-loop binding.
            (
                "fn f(s: &str) { for c in s.chars() { let _ = c as u32; } }",
                true,
            ),
            // Deref of a `&char` range accessor (the #5162 shape).
            (
                "fn f(range: std::ops::RangeInclusive<char>) -> u32 { *range.start() as u32 }",
                true,
            ),
            (
                "fn f(range: std::ops::RangeInclusive<char>) -> u32 { *range.end() as u32 }",
                true,
            ),
            // Parenthesized deref of a range accessor is still char.
            (
                "fn f(range: std::ops::RangeInclusive<char>) -> u32 { (*range.start()) as u32 }",
                true,
            ),
            // A deref of a non-accessor method is not recognized (sound: the
            // receiver type is unknown, so only the narrow accessor set qualifies).
            ("fn f(p: P) -> u32 { *p.value() as u32 }", false),
            // A method call with arguments is not a zero-arg accessor.
            ("fn f(p: P) -> u32 { *p.start(1) as u32 }", false),
            // A plain integer identifier is not a char operand.
            ("fn f(x: u32) -> u32 { x as u32 }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_of_kind(tree.root_node(), "type_cast_expression")
                .expect("snippet should contain a type_cast_expression");
            assert_eq!(
                cast_operand_is_char(cast, src.as_bytes()),
                expected,
                "cast_operand_is_char mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn cast_feeds_from_bits_matches_from_bits_argument() {
        let cases = [
            // Issue #5593: the cast is the argument to `f32::from_bits`.
            ("fn f() -> f32 { f32::from_bits(p as u32) }", true),
            ("fn f() -> f64 { f64::from_bits(p as u64) }", true),
            // A parenthesized wrapper between the cast and the arg list is
            // transparent.
            ("fn f() -> f32 { f32::from_bits((p as u32)) }", true),
            // Any receiver path's `from_bits` matches via the last segment.
            ("fn f() -> Flags { Flags::from_bits(x as u32) }", true),
            // A different associated function is not a bit-reinterpretation sink.
            ("fn f() -> u32 { u32::from(p as u32) }", false),
            // An ordinary call is not exempt.
            ("fn f(p: i32) -> u32 { consume(p as u32) }", false),
            // A bare cast not feeding any call is not exempt.
            ("fn f(p: i32) -> u32 { p as u32 }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_of_kind(tree.root_node(), "type_cast_expression")
                .expect("snippet should contain a type_cast_expression");
            assert_eq!(
                cast_feeds_from_bits(cast, src.as_bytes()),
                expected,
                "cast_feeds_from_bits mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn cast_is_int_to_float_matches_resolved_integer_to_float() {
        let cases = [
            // Issue #5690: integer params cast to a float target.
            ("fn f(n: u64) -> f32 { n as f32 }", true),
            ("fn f(n: i64) -> f64 { n as f64 }", true),
            ("fn f(n: u32) -> f32 { n as f32 }", true),
            ("fn f(n: i32) -> f32 { n as f32 }", true),
            // Platform-width integer sources have no `From<usize/isize>` for floats.
            ("fn f(n: usize) -> f64 { n as f64 }", true),
            ("fn f(n: isize) -> f64 { n as f64 }", true),
            // A deref/borrow integer source resolves through the referent.
            ("fn f(n: &u64) -> f32 { *n as f32 }", true),
            // Lossless integer -> float is still an integer source.
            ("fn f(n: u8) -> f32 { n as f32 }", true),
            // A typed-container element resolves as the source.
            ("fn f(b: &[u8]) -> f32 { b[0] as f32 }", true),
            // Float -> int is the reverse direction — not int -> float.
            ("fn f(x: f64) -> i32 { x as i32 }", false),
            // Float -> float narrowing has a float source.
            ("fn f(x: f64) -> f32 { x as f32 }", false),
            // Int -> int narrowing has a non-float target.
            ("fn f(x: i64) -> i32 { x as i32 }", false),
            // An unresolved operand is not proven to be an integer.
            ("fn f(s: S) -> f32 { s.value() as f32 }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_of_kind(tree.root_node(), "type_cast_expression")
                .expect("snippet should contain a type_cast_expression");
            assert_eq!(
                cast_is_int_to_float(cast, src.as_bytes()),
                expected,
                "cast_is_int_to_float mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn cast_feeds_sized_pointer_write_gates_matching_pointee_width() {
        let cases = [
            // Issue #5677: relocation-patch writes — the value cast's width
            // matches the destination pointer's pointee width.
            ("fn f() { unsafe { write_unaligned(a as *mut u8, v as u8); } }", true),
            ("fn f() { unsafe { write_unaligned(a as *mut u16, v as u16); } }", true),
            ("fn f() { unsafe { write_unaligned(a as *mut u32, v as u32); } }", true),
            // `ptr::write` and `write_volatile`, any receiver path, are covered.
            ("fn f() { unsafe { ptr::write(a as *mut u8, v as u8); } }", true),
            ("fn f() { unsafe { core::ptr::write_volatile(a as *mut u16, v as u16); } }", true),
            // A `*const` destination is recognized too.
            ("fn f() { unsafe { write(a as *const u32, v as u32); } }", true),
            // A parenthesized wrapper around the value cast is transparent.
            ("fn f() { unsafe { write_unaligned(a as *mut u8, (v as u8)); } }", true),
            // Width MISMATCH between pointee and value cast is NOT exempt.
            ("fn f() { unsafe { write_unaligned(a as *mut u8, v as u16); } }", false),
            ("fn f() { unsafe { write_unaligned(a as *mut u32, v as u8); } }", false),
            // The destination-pointer cast itself is never exempted by this
            // predicate (it is a pointer cast, not a numeric one, but guard it).
            ("fn f() { unsafe { write_unaligned(a as *mut u8, v as u8); } }", true),
            // A non-write call with the same shape is not exempt.
            ("fn f() { consume(a as *mut u8, v as u8); }", false),
            // A write whose destination is not a typed pointer cast is not exempt.
            ("fn f() { unsafe { write_unaligned(dst, v as u8); } }", false),
            // A bare lossy cast feeding no call still flags.
            ("fn f(x: u64) -> u8 { x as u8 }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            // The value cast is the LAST `type_cast_expression` whose target is a
            // plain numeric type; grab the numeric (non-pointer) cast.
            let cast = numeric_cast(tree.root_node(), src.as_bytes())
                .expect("snippet should contain a numeric type_cast_expression");
            assert_eq!(
                cast_feeds_sized_pointer_write(cast, src.as_bytes()),
                expected,
                "cast_feeds_sized_pointer_write mismatch for `{src}`"
            );
        }
    }

    /// Find the first `type_cast_expression` whose target is NOT a pointer type —
    /// i.e. the numeric value cast, skipping `addr as *mut uN` destination casts.
    fn numeric_cast<'tree>(node: Node<'tree>, source: &[u8]) -> Option<Node<'tree>> {
        if node.kind() == "type_cast_expression"
            && node.child_by_field_name("type").is_some_and(|t| {
                fixed_width_int_kind(t.utf8_text(source).unwrap_or("").trim()).is_some()
            })
        {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = numeric_cast(child, source) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn cast_feeds_simd_intrinsic_gates_same_width_signed_unsigned() {
        let cases = [
            // Issue #5600: unresolved `u64` source cast to the `i64` lane of
            // `_mm_set_epi64x` (the first cast in the call).
            ("fn f() -> M { _mm_set_epi64x(hi as i64, lo as i64) }", true),
            ("fn f() -> M { _mm_set1_epi64x(0x8040u64 as i64) }", true),
            // Resolved same-width signed↔unsigned reinterpretation.
            ("fn f(x: u32) -> M { _mm_set1_epi32(x as i32) }", true),
            ("fn f(x: i32) -> M { _mm_set1_epi32(x as u32) }", true),
            // AVX2/AVX-512 register-width prefixes are recognized too.
            ("fn f(x: u32) -> M { _mm256_set1_epi32(x as i32) }", true),
            ("fn f(x: u64) -> M { _mm512_set1_epi64(x as i64) }", true),
            // A parenthesized wrapper is transparent.
            ("fn f() -> M { _mm_set1_epi32((load() as i32)) }", true),
            // Resolved NARROWING into the lane type is not same-width — flagged.
            ("fn f(x: u64) -> M { _mm_set1_epi32(x as i32) }", false),
            // Same signedness (not a sign reinterpretation) is not exempt.
            ("fn f(x: u64) -> M { _mm_set1_epi32(x as u32) }", false),
            // A non-SIMD call is not exempt.
            ("fn f(x: u64) -> i64 { consume(x as i64) }", false),
            // `usize`/`isize` targets are platform-width — never same-width.
            ("fn f(x: u64) -> M { _mm_set1_epi64(x as isize) }", false),
            // A RESOLVED platform-width source must NOT fall through to the
            // lane-width fallback: `usize as i32` is narrowing on 64-bit targets.
            ("fn f(x: usize) -> M { _mm_set1_epi32(x as i32) }", false),
            // A bare cast feeding no call is not exempt.
            ("fn f(x: u64) -> i64 { x as i64 }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_of_kind(tree.root_node(), "type_cast_expression")
                .expect("snippet should contain a type_cast_expression");
            assert_eq!(
                cast_feeds_simd_intrinsic(cast, src.as_bytes()),
                expected,
                "cast_feeds_simd_intrinsic mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn cast_operand_is_ascii_guarded_requires_matching_is_ascii_check() {
        let cases = [
            // Issue #5122 (chumsky): `.is_ascii().then_some(*self as u8)` — the
            // deref'd operand `*self` is guarded by `self.is_ascii()`.
            (
                "fn to_ascii(&self) -> Option<u8> { self.is_ascii().then_some(*self as u8) }",
                true,
            ),
            // Bare identifier operand and receiver.
            (
                "fn f(ch: char) -> Option<u8> { ch.is_ascii().then_some(ch as u8) }",
                true,
            ),
            // `.then(|| ..)` form.
            (
                "fn f(ch: char) -> Option<u8> { ch.is_ascii().then(|| ch as u8) }",
                true,
            ),
            // `if` consequence form.
            (
                "fn f(ch: char) -> Option<u8> { if ch.is_ascii() { Some(ch as u8) } else { None } }",
                true,
            ),
            // An `is_ascii_*` variant (subset of ASCII) also proves the range.
            (
                "fn f(ch: char) -> Option<u8> { ch.is_ascii_digit().then_some(ch as u8) }",
                true,
            ),
            // Widening into a wider integer is equally exempt.
            (
                "fn f(ch: char) -> Option<i32> { ch.is_ascii().then_some(ch as i32) }",
                true,
            ),
            // No guard at all.
            ("fn f(ch: char) -> u8 { ch as u8 }", false),
            // Guard tests a DIFFERENT value than the one cast.
            (
                "fn f(a: char, b: char) -> Option<u8> { a.is_ascii().then_some(b as u8) }",
                false,
            ),
            // An unrelated predicate (not `is_ascii*`) does not prove the range.
            (
                "fn f(ch: char) -> Option<u8> { ch.is_alphabetic().then_some(ch as u8) }",
                false,
            ),
            // Guard reached only through the `else` branch is the negation.
            (
                "fn f(ch: char) -> Option<u8> { if ch.is_ascii() { None } else { Some(ch as u8) } }",
                false,
            ),
            // `then_some`-receiver guards a different value (deref mismatch).
            (
                "fn f(p: &char, q: char) -> Option<u8> { p.is_ascii().then_some(q as u8) }",
                false,
            ),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_of_kind(tree.root_node(), "type_cast_expression")
                .expect("snippet should contain a type_cast_expression");
            assert_eq!(
                cast_operand_is_ascii_guarded(cast, src.as_bytes()),
                expected,
                "cast_operand_is_ascii_guarded mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn cast_operand_is_range_guarded_requires_dominating_upper_bound() {
        let cases = [
            // Canonical msgpack pattern: each arm is guarded.
            ("fn w(val: u64) -> u8 { if val < 256 { val as u8 } else { 0 } }", true),
            ("fn w(val: u64) -> u16 { if val < 65536 { val as u16 } else { 0 } }", true),
            // Inclusive bound at exactly T::MAX.
            ("fn w(val: u64) -> u8 { if val <= 255 { val as u8 } else { 0 } }", true),
            // Symmetric `N > val` / `N >= val` forms.
            ("fn w(val: u64) -> u8 { if 256 > val { val as u8 } else { 0 } }", true),
            ("fn w(val: u64) -> u8 { if 255 >= val { val as u8 } else { 0 } }", true),
            // Digit separators in the literal.
            ("fn w(val: u64) -> u16 { if val < 65_536 { val as u16 } else { 0 } }", true),
            // No guard at all.
            ("fn f(n: u64) -> u8 { n as u8 }", false),
            // Bound exceeds the target's range.
            ("fn w(val: u64) -> u8 { if val < 1000 { val as u8 } else { 0 } }", false),
            ("fn w(val: u64) -> u8 { if val <= 256 { val as u8 } else { 0 } }", false),
            // Signed source: an upper bound does not rule out a negative value.
            ("fn w(val: i64) -> u8 { if val < 256 { val as u8 } else { 0 } }", false),
            // Unresolved source type: cannot prove non-negativity.
            ("fn w(val: T) -> u8 { if val < 256 { val as u8 } else { 0 } }", false),
            // Guard is on a different variable.
            ("fn w(a: u64, b: u64) -> u8 { if a < 256 { b as u8 } else { 0 } }", false),
            // Guard reached through the `else` branch is the condition's negation.
            ("fn w(val: u64) -> u8 { if val < 256 { 0 } else { val as u8 } }", false),
            // Signed target is out of scope (lower bound not provable here).
            ("fn w(val: u64) -> i8 { if val < 128 { val as i8 } else { 0 } }", false),
            // A lower-bound guard does not bound the cast from above.
            ("fn w(val: u64) -> u8 { if val > 10 { val as u8 } else { 0 } }", false),
            // u128 target: the bound fits, and `2^128 - 1` must not overflow.
            ("fn w(val: u64) -> u128 { if val < 256 { val as u128 } else { 0 } }", true),
            // Shadowing `let val` inside the branch: the guard bounds the OUTER
            // `val`, not the inner one the cast reads.
            ("fn w(val: u64) -> u8 { if val < 256 { let val: u64 = q(); val as u8 } else { 0 } }", false),
            // Reassignment after the guard invalidates the proven bound.
            ("fn w(mut val: u64) -> u8 { if val < 256 { val = 9999; val as u8 } else { 0 } }", false),
            // Compound assignment likewise invalidates the bound.
            ("fn w(mut val: u64) -> u8 { if val < 256 { val += 9999; val as u8 } else { 0 } }", false),
            // A write AFTER the cast does not invalidate it.
            ("fn w(mut val: u64) -> u8 { if val < 256 { let r = val as u8; val = 9999; r } else { 0 } }", true),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_of_kind(tree.root_node(), "type_cast_expression")
                .expect("snippet should contain a type_cast_expression");
            assert_eq!(
                cast_operand_is_range_guarded(cast, src.as_bytes()),
                expected,
                "cast_operand_is_range_guarded mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn cast_operand_is_range_guarded_handles_else_and_exit_guard() {
        let cases = [
            // --- `else`-branch upper bound (issue #6173) ---
            // The else is reached only when `value <= 100`, which fits u8.
            ("fn n(value: u32) -> u8 { if value > 100 { 0 } else { value as u8 } }", true),
            // `>=` negates to an exclusive `< N` upper bound.
            ("fn n(value: u32) -> u8 { if value >= 256 { 0 } else { value as u8 } }", true),
            // Inclusive `>` at exactly T::MAX.
            ("fn n(value: u32) -> u8 { if value > 255 { 0 } else { value as u8 } }", true),
            // Mirrored `N < val` / `N <= val` forms.
            ("fn n(value: u32) -> u8 { if 100 < value { 0 } else { value as u8 } }", true),
            ("fn n(value: u32) -> u8 { if 256 <= value { 0 } else { value as u8 } }", true),
            // else-if chain: the outer `value > N` negation reaches the final else.
            (
                "fn n(value: u32, f: bool) -> u8 { if value > 100 { 0 } else if f { 1 } else { value as u8 } }",
                true,
            ),
            // Bound too large for the target: the else does not prove a fit.
            ("fn n(value: u32) -> u8 { if value > 256 { 0 } else { value as u8 } }", false),
            ("fn n(value: u32) -> u8 { if value >= 257 { 0 } else { value as u8 } }", false),
            // An upper-bound condition leaves only a LOWER bound in the else.
            ("fn n(value: u32) -> u8 { if value < 100 { 0 } else { value as u8 } }", false),
            // else-branch guard on a different variable.
            (
                "fn n(value: u32, other: u32) -> u8 { if other > 100 { 0 } else { value as u8 } }",
                false,
            ),
            // Reassignment inside the else before the cast invalidates the bound.
            (
                "fn n(mut value: u32) -> u8 { if value > 100 { 0 } else { value = 9999; value as u8 } }",
                false,
            ),
            // Signed source: an upper bound cannot rule out a negative value.
            ("fn n(value: i32) -> u8 { if value > 100 { 0 } else { value as u8 } }", false),

            // --- preceding early-exit guard (fallthrough) ---
            ("fn g(value: u32) -> u8 { if value > 100 { return 0; } value as u8 }", true),
            ("fn g(value: u32) -> u8 { if value > 100 { panic!(\"too big\"); } value as u8 }", true),
            // Diverging tail without a trailing `;`.
            ("fn g(value: u32) -> u8 { if value > 100 { unreachable!() } value as u8 }", true),
            // Non-diverging then-branch: the large value flows through.
            ("fn g(value: u32) -> u8 { if value > 100 { let _x = 1; } value as u8 }", false),
            // A guard with an `else` is not a pure early-exit guard.
            (
                "fn g(value: u32) -> u8 { if value > 100 { return 0; } else { } value as u8 }",
                false,
            ),
            // Guard on a different variable.
            (
                "fn g(value: u32, other: u32) -> u8 { if other > 100 { return 0; } value as u8 }",
                false,
            ),
            // Reassignment between the guard and the cast invalidates the bound.
            (
                "fn g(mut value: u32) -> u8 { if value > 100 { return 0; } value = 9999; value as u8 }",
                false,
            ),
            // Bound too large for the target.
            ("fn g(value: u32) -> u8 { if value > 256 { return 0; } value as u8 }", false),
            // Signed source.
            ("fn g(value: i32) -> u8 { if value > 100 { return 0; } value as u8 }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_of_kind(tree.root_node(), "type_cast_expression")
                .expect("snippet should contain a type_cast_expression");
            assert_eq!(
                cast_operand_is_range_guarded(cast, src.as_bytes()),
                expected,
                "cast_operand_is_range_guarded mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn cast_operand_is_min_clamped_proves_bound() {
        let cases = [
            // --- proof 1: unsigned-typed bound (issue #6174) ---
            // Canonical tonic pattern: `.min(u64::MAX as u128) as u64`.
            ("fn f(x: u128) -> u64 { x.min(u64::MAX as u128) as u64 }", true),
            // Symmetric form, bound type already matches the target.
            ("fn f(x: u64) -> u64 { x.min(u64::MAX) as u64 }", true),
            // `(n as u64).min(u32::MAX as u64) as u32`.
            ("fn f(n: u32) -> u32 { (n as u64).min(u32::MAX as u64) as u32 }", true),
            // Parens around the whole clamped operand are transparent.
            ("fn f(x: u128) -> u64 { (x.min(u64::MAX as u128)) as u64 }", true),
            // An unsigned-cast literal bound (`200 as u64`) is typed + in range.
            ("fn f(x: u64) -> u8 { x.min(200 as u64) as u8 }", true),
            // The unsigned-cast bound's inner value is read exactly: `1000 as u64`
            // is typed-unsigned but exceeds u8, so it stays flagged.
            ("fn f(x: u64) -> u8 { x.min(1000 as u64) as u8 }", false),
            // Parens around a bare literal are not unwrapped in proof 2 (no type
            // proof); conservatively kept flagging.
            ("fn f(v: u32) -> u8 { v.min((255)) as u8 }", false),

            // --- proof 2: bare literal bound + provably non-negative receiver ---
            ("fn f(v: u32) -> u8 { v.min(255) as u8 }", true),
            ("fn f(v: u32) -> u8 { v.min(0xFF) as u8 }", true),

            // --- negative space: must keep flagging ---
            // No `.min()` clamp at all.
            ("fn f(n: u64) -> u8 { n as u8 }", false),
            // Typed bound exceeds the target's range.
            ("fn f(x: u128) -> u8 { x.min(u64::MAX as u128) as u8 }", false),
            // Bare-literal bound exceeds the target's range.
            ("fn f(v: u32) -> u8 { v.min(300) as u8 }", false),
            // Wrong direction: `.max()` does not bound from above.
            ("fn f(x: u128) -> u64 { x.max(u64::MAX as u128) as u64 }", false),
            // Signed receiver, bare literal: `.min()` cannot rule out a negative
            // value; `(-1i64).min(255) as u8` wraps to 255.
            ("fn f(x: i64) -> u8 { x.min(255) as u8 }", false),
            // Unresolved receiver type with a bare literal bound.
            ("fn f(x: T) -> u8 { x.min(255) as u8 }", false),
            // Signed target is never exempt (lower bound below T::MIN unprovable).
            ("fn f(x: u64) -> i8 { x.min(127) as i8 }", false),
            // A signed type's `::MAX` is not an unsigned-typed bound.
            ("fn f(x: i64) -> u8 { x.min(i64::MAX) as u8 }", false),
            // A non-`MAX` const bound carries no statically-known value.
            ("fn f(x: u64) -> u8 { x.min(LIMIT) as u8 }", false),
            // Zero-argument `.min()` (no clamp bound).
            ("fn f(x: u64) -> u8 { x.min() as u8 }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_of_kind(tree.root_node(), "type_cast_expression")
                .expect("snippet should contain a type_cast_expression");
            assert_eq!(
                cast_operand_is_min_clamped(cast, src.as_bytes()),
                expected,
                "cast_operand_is_min_clamped mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn cast_operand_is_non_negative_guarded_requires_dominating_proof() {
        let cases = [
            // Match-arm guard, unresolved source, widening into u64 (issue #5262).
            (
                "fn f(o: Option<i64>) -> Option<u64> { match o { Some(diff) if !diff.is_negative() => Some(diff as u64), _ => None } }",
                true,
            ),
            // `if x >= 0` with a resolved signed source, equal-width unsigned.
            ("fn f(x: i32) -> u32 { if x >= 0 { x as u32 } else { 0 } }", true),
            // `x.is_positive()` guard, signed widening.
            ("fn f(x: i32) -> u64 { if x.is_positive() { x as u64 } else { 0 } }", true),
            // `x > 0` and `x > -1` both prove non-negativity.
            ("fn f(x: i32) -> u32 { if x > 0 { x as u32 } else { 0 } }", true),
            ("fn f(x: i32) -> u32 { if x > -1 { x as u32 } else { 0 } }", true),
            // Mirrored `0 <= x` / `0 < x` / `-1 < x` (identifier on the right).
            ("fn f(x: i32) -> u32 { if 0 <= x { x as u32 } else { 0 } }", true),
            ("fn f(x: i32) -> u32 { if 0 < x { x as u32 } else { 0 } }", true),
            ("fn f(x: i32) -> u32 { if -1 < x { x as u32 } else { 0 } }", true),
            // `!x.is_negative()` at the if-expression site (not only match arms).
            ("fn f(x: i32) -> u32 { if !x.is_negative() { x as u32 } else { 0 } }", true),
            // Shadowing `let x` inside the branch reads a different binding.
            ("fn f(x: i32) -> u32 { if x >= 0 { let x: i32 = q(); x as u32 } else { 0 } }", false),
            // No guard at all.
            ("fn f(x: i32) -> u32 { x as u32 }", false),
            // Guarded narrowing: a non-negative i64 can still exceed u8.
            ("fn f(x: i64) -> u8 { if x >= 0 { x as u8 } else { 0 } }", false),
            // Unresolved source narrowing into u8 is not the widening idiom.
            (
                "fn f(o: Option<i64>) -> Option<u8> { match o { Some(d) if !d.is_negative() => Some(d as u8), _ => None } }",
                false,
            ),
            // Guard reached through the `else` branch is the condition's negation.
            ("fn f(x: i32) -> u32 { if x >= 0 { 0 } else { x as u32 } }", false),
            // Guard on a different variable.
            ("fn f(a: i32, b: i32) -> u32 { if a >= 0 { b as u32 } else { 0 } }", false),
            // An upper-bound guard does not prove non-negativity.
            ("fn f(x: i32) -> u32 { if x < 256 { x as u32 } else { 0 } }", false),
            // Reassignment after the guard invalidates the proof.
            ("fn f(mut x: i32) -> u32 { if x >= 0 { x = -9; x as u32 } else { 0 } }", false),
            // Target is signed — out of scope (this predicate is signed→unsigned).
            ("fn f(x: i32) -> i64 { if x >= 0 { x as i64 } else { 0 } }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_of_kind(tree.root_node(), "type_cast_expression")
                .expect("snippet should contain a type_cast_expression");
            assert_eq!(
                cast_operand_is_non_negative_guarded(cast, src.as_bytes()),
                expected,
                "cast_operand_is_non_negative_guarded mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn is_inside_async_fn_distinguishes_async_from_raw_identifier() {
        let cases = [
            ("async fn f() { g(); }", true),
            ("pub async fn f() { g(); }", true),
            ("fn r#async() { g(); }", false),
            ("fn f() { g(); }", false),
            ("fn f(r#async: u8) { g(); }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let call = first_call_expression(tree.root_node())
                .expect("snippet should contain a call_expression");
            assert_eq!(
                is_inside_async_fn(call, src.as_bytes()),
                expected,
                "is_inside_async_fn mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn enclosing_fn_finds_nearest_function_or_none() {
        // Inside a function body: the call's enclosing fn is found.
        let src = "fn outer() { inner(); }";
        let tree = parse(src);
        let call = first_call_expression(tree.root_node())
            .expect("snippet should contain a call_expression");
        assert!(enclosing_fn(call).is_some_and(|f| f.kind() == "function_item"));

        // At module scope (a const initializer): no enclosing function.
        let src = "const X: u32 = compute();";
        let tree = parse(src);
        let call = first_call_expression(tree.root_node())
            .expect("snippet should contain a call_expression");
        assert!(enclosing_fn(call).is_none());
    }

    #[test]
    fn is_in_kani_proof_recognizes_proof_harnesses() {
        let cases = [
            // `#[kani::proof]` on the enclosing fn.
            ("#[kani::proof] fn h() { f(); }", true),
            // `#[kani::proof_for_contract(...)]` on the enclosing fn.
            (
                "#[kani::proof_for_contract(crate::Epoch::weekday)] fn h() { f(); }",
                true,
            ),
            // A Kani attribute on an inner closure's host fn still covers a
            // nested call.
            ("#[kani::proof] fn h() { let g = || { f(); }; }", true),
            // No Kani attribute: a plain fn is not a harness.
            ("fn h() { f(); }", false),
            // A different `kani` attribute that is not a proof harness.
            ("#[kani::requires(x > 0)] fn h() { f(); }", false),
            // A non-kani attribute whose name merely ends in `proof`.
            ("#[my::proof] fn h() { f(); }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let call = first_call_expression(tree.root_node())
                .expect("snippet should contain a call_expression");
            assert_eq!(
                is_in_kani_proof(call, src.as_bytes()),
                expected,
                "is_in_kani_proof mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn is_in_loop_body_respects_scope_boundaries() {
        let cases = [
            // Directly inside each loop form.
            ("fn f() { loop { g(); } }", true),
            ("fn f() { while c { g(); } }", true),
            ("fn f() { for x in xs { g(); } }", true),
            // Not in any loop.
            ("fn f() { g(); }", false),
            // A loop nested BELOW the call (call is above the loop) — not seen.
            ("fn f() { g(); loop { h(); } }", false),
            // A closure boundary between the loop and the call: the call lives
            // in the closure, not in the loop body proper.
            ("fn f() { for x in xs { register(|| { g(); }); } }", false),
            // An async-block boundary (spawned future) between loop and call.
            ("fn f() { for x in xs { spawn(async { g(); }); } }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            // Anchor on the `g()` / `h()` call we care about: the first call
            // whose callee identifier is `g` or `h`.
            let mut calls = Vec::new();
            collect_calls(tree.root_node(), &mut calls);
            let target = calls
                .into_iter()
                .find(|c| {
                    c.child_by_field_name("function")
                        .and_then(|f| f.utf8_text(src.as_bytes()).ok())
                        .is_some_and(|t| t == "g" || t == "h")
                })
                .expect("snippet should contain a `g()` or `h()` call");
            assert_eq!(
                is_in_loop_body(target),
                expected,
                "is_in_loop_body mismatch for `{src}`"
            );
        }
    }

    /// Collect every `call_expression` node in the subtree, pre-order.
    fn collect_calls<'tree>(node: Node<'tree>, out: &mut Vec<Node<'tree>>) {
        if node.kind() == "call_expression" {
            out.push(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_calls(child, out);
        }
    }

    #[test]
    fn has_outer_attribute_matches_path_segment_only() {
        let cases = [
            ("#[track_caller]\nfn f() {}", "track_caller", true),
            ("#[core::track_caller]\nfn f() {}", "track_caller", true),
            ("#[inline]\n#[track_caller]\nfn f() {}", "track_caller", true),
            // A comment between the attribute and the item must not defeat it.
            ("#[track_caller]\n// note\nfn f() {}", "track_caller", true),
            // No such attribute.
            ("#[inline]\nfn f() {}", "track_caller", false),
            ("fn f() {}", "track_caller", false),
            // A different attribute whose name merely ends in the path.
            ("#[my_track_caller]\nfn f() {}", "track_caller", false),
        ];
        for (src, attr, expected) in cases {
            let tree = parse(src);
            let item = first_function_item(tree.root_node())
                .expect("snippet should contain a function_item");
            assert_eq!(
                has_outer_attribute(item, src.as_bytes(), attr),
                expected,
                "has_outer_attribute mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn has_panics_doc_section_matches_rustdoc_heading_only() {
        let cases = [
            ("/// # Panics\nfn f() {}", true),
            // Heading body and blank doc lines around it.
            ("/// Does a thing.\n///\n/// # Panics\n///\n/// when bad.\nfn f() {}", true),
            // `## Panics` (deeper heading) also counts.
            ("/// ## Panics\nfn f() {}", true),
            // A `#[track_caller]` attribute between the doc and the fn is skipped.
            ("/// # Panics\n#[track_caller]\nfn f() {}", true),
            // Block doc comment carrying the heading.
            ("/** # Panics\n\n may panic. */\nfn f() {}", true),
            // Prose that merely mentions panics is not a heading.
            ("/// Panics if bad.\nfn f() {}", false),
            ("/// This may panic.\nfn f() {}", false),
            // A non-doc `//` comment is ignored even with the heading text.
            ("// # Panics\nfn f() {}", false),
            // No doc at all.
            ("fn f() {}", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let item = first_function_item(tree.root_node())
                .expect("snippet should contain a function_item");
            assert_eq!(
                has_panics_doc_section(item, src.as_bytes()),
                expected,
                "has_panics_doc_section mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn fn_is_async_distinguishes_async_from_sync() {
        let cases = [
            ("async fn f() {}", true),
            ("fn f() {}", false),
            ("const fn f() {}", false),
            // Raw identifier named `async` is a sync fn.
            ("fn r#async() {}", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let item = first_function_item(tree.root_node())
                .expect("snippet should contain a function_item");
            assert_eq!(
                fn_is_async(item, src.as_bytes()),
                expected,
                "fn_is_async mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn collect_top_level_derives_only_reads_top_level_derive() {
        let cases: [(&str, &[&str]); 6] = [
            // Plain top-level derive.
            ("#[derive(Ord, PartialEq, Eq)]\nstruct A;", &["Ord", "PartialEq", "Eq"]),
            // Several top-level derives accumulate (collected nearest-first,
            // walking preceding siblings in reverse; order is irrelevant to
            // callers, which use `.iter().any(...)`).
            ("#[derive(Clone)]\n#[derive(Hash)]\nstruct A;", &["Hash", "Clone"]),
            // A nested `derive(` inside `rkyv(...)` inside `cfg_attr(...)` is
            // NOT a top-level derive on the host — issue #3944.
            (
                "#[derive(Clone)]\n#[cfg_attr(feature = \"rkyv\", rkyv(derive(Debug, Eq, PartialEq, PartialOrd, Ord)))]\nstruct A;",
                &["Clone"],
            ),
            // A cfg-gated `derive(` is conditional, not unconditional top-level:
            // collected only when its path is `derive`, and here the path is
            // `cfg_attr`, so it is ignored (the conservative #3944 direction).
            ("#[cfg_attr(feature = \"x\", derive(Hash))]\nstruct A;", &[]),
            // No derives at all.
            ("#[repr(C)]\nstruct A;", &[]),
            ("struct A;", &[]),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let item = first_of_kind(tree.root_node(), "struct_item")
                .expect("snippet should contain a struct_item");
            assert_eq!(
                collect_top_level_derives(item, src.as_bytes()),
                expected,
                "collect_top_level_derives mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn subtree_string_literal_contains_matches_byte_and_raw_strings() {
        let cases = [
            // Plain string literal.
            (r#"fn f() { let _ = "needle here"; }"#, "needle", true),
            // Byte-string literal (`b"…"`) — still a `string_literal` node.
            (r#"fn f() { g(&b"abc-NEEDLE-def"[..]); }"#, "needle", true),
            // Raw string literal.
            (r##"fn f() { let _ = r#"a needle b"#; }"##, "needle", true),
            // Case-insensitive match.
            (r#"fn f() { let _ = "ABC123"; }"#, "abc123", true),
            // The needle is an identifier, not a literal → no match.
            (r#"fn f() { let needle = 1; }"#, "needle", false),
            // Absent.
            (r#"fn f() { let _ = "other"; }"#, "needle", false),
        ];
        for (src, needle, expected) in cases {
            let tree = parse(src);
            assert_eq!(
                subtree_string_literal_contains(tree.root_node(), src.as_bytes(), needle),
                expected,
                "subtree_string_literal_contains mismatch for `{src}` / `{needle}`"
            );
        }
    }

    /// Find the first `enum_item` node anywhere in the tree.
    fn first_enum_item(node: Node) -> Option<Node> {
        if node.kind() == "enum_item" {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = first_enum_item(child) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn enum_has_cfg_gated_variant_detects_gated_and_plain_enums() {
        let cases = [
            // The poem `Addr` repro: a `#[cfg(unix)]`-gated variant.
            (
                "enum Addr { SocketAddr(S), #[cfg(unix)] Unix(U), Custom(C) }",
                true,
            ),
            // `#[cfg_attr(...)]` gating also makes the variant set
            // target-dependent.
            (
                "enum E { A, #[cfg_attr(feature = \"x\", cfg(unix))] B }",
                true,
            ),
            // Comment between the attribute and the variant must not defeat it.
            ("enum E { A, #[cfg(unix)]\n// note\nB }", true),
            // No cfg attribute anywhere — exhaustive listing is portable.
            ("enum E { A, B, C }", false),
            // A non-cfg attribute (`#[serde(rename)]`) must not count.
            ("enum E { A, #[serde(rename = \"b\")] B }", false),
            // An identifier merely ending in `cfg` is not `cfg`.
            ("enum E { A, #[mycfg(unix)] B }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let enum_item = first_enum_item(tree.root_node()).expect("enum present");
            assert_eq!(
                enum_has_cfg_gated_variant(enum_item, src.as_bytes()),
                expected,
                "enum_has_cfg_gated_variant mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn is_under_cfg_debug_assertions_distinguishes_debug_gate_from_other_cfgs() {
        let cases = [
            // The gated statement itself — compiles out in release.
            (
                "fn f() { #[cfg(debug_assertions)] foo().unwrap(); bar() }",
                true,
            ),
            // A comment between the gate and the statement must not defeat it.
            (
                "fn f() { #[cfg(debug_assertions)]\n// note\nfoo().unwrap(); }",
                true,
            ),
            // Gated `let` binding — the unwrap is still under the gate.
            (
                "fn f() { #[cfg(debug_assertions)] let _ = foo().unwrap(); }",
                true,
            ),
            // No cfg gate at all — a real runtime unwrap.
            ("fn f() { foo().unwrap(); }", false),
            // A `#[cfg(feature = \"x\")]` gate is a real release path.
            (
                "fn f() { #[cfg(feature = \"x\")] foo().unwrap(); }",
                false,
            ),
            // `#[cfg(not(debug_assertions))]` is release-only: `debug_assertions`
            // is nested in `not(...)`, not a direct child of the `cfg` tree.
            (
                "fn f() { #[cfg(not(debug_assertions))] foo().unwrap(); }",
                false,
            ),
            // An unrelated attribute (`#[allow(...)]`) is not a debug gate.
            ("fn f() { #[allow(unused)] foo().unwrap(); }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let call = first_unwrap_call(tree.root_node(), src.as_bytes())
                .expect("unwrap call present");
            assert_eq!(
                is_under_cfg_debug_assertions(call, src.as_bytes()),
                expected,
                "is_under_cfg_debug_assertions mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn local_let_binds_vec_confirms_vec_shapes_and_rejects_others() {
        // Anchor on the `for_expression`, mirroring how the caller passes the
        // node whose enclosing scopes are searched for the `var` binding.
        let cases = [
            ("fn f(src: Vec<u32>) { let v = Vec::new(); for x in src { v.push(x); } }", "v", true),
            ("fn f(src: Vec<u32>) { let v = vec![]; for x in src { v.push(x); } }", "v", true),
            ("fn f(src: Vec<u32>) { let v = Vec::with_capacity(4); for x in src { v.push(x); } }", "v", true),
            ("fn f(src: Vec<u32>) { let v: Vec<u32> = make(); for x in src { v.push(x); } }", "v", true),
            ("fn f(src: Vec<u32>) { let mut v = Vec::new(); for x in src { v.push(x); } }", "v", true),
            // A parameter binding is not confirmed here — only a `let`.
            ("fn f(src: Vec<u32>, v: Vec<u32>) { for x in src { v.push(x); } }", "v", false),
            // Non-`Vec` initializer / annotation.
            ("fn f(src: Vec<u32>) { let v = Queue::new(); for x in src { v.push(x); } }", "v", false),
            ("fn f(src: Vec<u32>) { let v: Queue<u32> = make(); for x in src { v.push(x); } }", "v", false),
            // The `let` must lexically precede the loop in its block.
            ("fn f(src: Vec<u32>) { for x in src { v.push(x); } let v = Vec::new(); }", "v", false),
        ];
        for (src, var, expected) in cases {
            let tree = parse(src);
            let for_node = first_of_kind(tree.root_node(), "for_expression")
                .expect("snippet should contain a for_expression");
            assert_eq!(
                local_let_binds_vec(for_node, var, src.as_bytes()),
                expected,
                "local_let_binds_vec mismatch for `{src}`"
            );
        }
    }

    #[test]
    fn cast_operand_is_raw_pointer_recognizes_pointer_sources() {
        let cases = [
            // Inner `as *const _` / `as *mut <int>` cast operand → pointer source.
            ("fn f() { let _ = executor as *const _ as u32; }", true),
            ("fn f() { let _ = &mut self.table as *mut _ as *mut u32 as u32; }", true),
            // `.as_ptr()` / `.as_mut_ptr()` method call operand.
            ("fn f() { let _ = task.as_ptr() as u32; }", true),
            ("fn f() { let _ = regs.ch().cc().as_ptr() as u32; }", true),
            ("fn f() { let _ = region.as_mut_ptr() as usize; }", true),
            // `ptr::null()` / `null_mut()` operand, including turbofish.
            ("fn f() { let _ = core::ptr::null::<u8>() as u32; }", true),
            ("fn f() { let _ = ptr::null_mut() as u32; }", true),
            // Parenthesized pointer operand stays transparent.
            ("fn f() { let _ = (task.as_ptr()) as u32; }", true),
            // Plain integer / non-pointer operands are NOT pointer sources.
            ("fn f() { let _ = len as u32; }", false),
            ("fn f() { let _ = x as u8; }", false),
            ("fn f() { let _ = buf.len() as u32; }", false),
            // A non-pointer inner cast must not match (`x as u64 as u32`).
            ("fn f() { let _ = x as u64 as u32; }", false),
        ];
        for (src, expected) in cases {
            let tree = parse(src);
            let cast = first_type_cast_expression(tree.root_node())
                .expect("snippet should contain a type_cast_expression");
            assert_eq!(
                cast_operand_is_raw_pointer(cast, src.as_bytes()),
                expected,
                "cast_operand_is_raw_pointer mismatch for `{src}`"
            );
        }
    }
}
