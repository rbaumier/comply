//! Shared helpers for Rust tree-sitter rules.
//!
//! Extracted because three independent rules need the same
//! "are we inside an async function" check (`thread-sleep-in-async`,
//! `block-on-in-async`, `sync-io-in-async`). Rule of three: extract.

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
            return fn_modifiers_contain_async(parent, source);
        }
        cur = parent;
    }
    false
}

/// True if a `function_item`'s `function_modifiers` child contains the
/// `async` keyword. Scans the modifiers node only, so raw identifiers
/// (`fn r#async()`), parameter types, and return types named "async"
/// can't trip the check.
fn fn_modifiers_contain_async(function_item: Node, source: &[u8]) -> bool {
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

/// True if `path` is test infrastructure recognizable by path or file name,
/// independent of any `#[cfg(test)]` attribute.
///
/// A file qualifies when either:
///
/// - any path SEGMENT (exact component match) is `tests`, `property_tests`,
///   `test_utils`, `test_helpers`, or `testing` — covers Cargo's `tests/`
///   integration directory, `property_tests/` generators, and shared
///   test-helper modules at any nesting depth; OR
/// - the file NAME is exactly `testing.rs`, `test_utils.rs`, or
///   `test_helpers.rs`.
///
/// Cross-crate test helpers cannot be `#[cfg(test)]` (that gate hides them
/// from integration tests in *other* crates), so their test-only nature is
/// conveyed by path and name instead. Matching is on exact segments / exact
/// file names, never substrings: `testingground/` and `my_testing.rs` are
/// production code and do not qualify.
///
/// Shared by Rust rules that relax their discipline (allow `unwrap`,
/// `panic!`, …) for test infrastructure without relying on the tree-sitter
/// attribute walk.
pub fn is_under_tests_dir(path: &std::path::Path) -> bool {
    const TEST_SEGMENTS: &[&str] = &[
        "tests",
        "property_tests",
        "test_utils",
        "test_helpers",
        "testing",
    ];
    const TEST_FILE_NAMES: &[&str] = &["testing.rs", "test_utils.rs", "test_helpers.rs"];

    if path
        .components()
        .any(|c| TEST_SEGMENTS.iter().any(|seg| c.as_os_str() == *seg))
    {
        return true;
    }
    path.file_name()
        .is_some_and(|name| TEST_FILE_NAMES.iter().any(|test_name| name == *test_name))
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
pub fn has_test_attribute(item: Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && attr_marks_test(text)
        {
            return true;
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

/// If a `cfg(` / `cfg_attr(` token begins at or after the byte cursor `*i`,
/// advance `*i` past the keyword and opening paren and return the index of the
/// first byte inside the parentheses. Otherwise advance `*i` by one and return
/// `None`.
fn cfg_arg_open(text: &str, i: &mut usize) -> Option<usize> {
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

/// True if the `match_arm`'s body is a single diverging or error
/// expression — a `unreachable!`/`panic!`/`unimplemented!`/`todo!`/`bail!`
/// macro invocation, or a `return Err(...)`. Such an arm is an explicit
/// guard for the impossible/error case.
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
        "return_expression" => return_yields_err(expr, source),
        _ => false,
    }
}

/// True if a `return_expression` returns an `Err(...)` value — the head
/// of the returned call expression is the `Err` constructor.
fn return_yields_err(ret: Node, source: &[u8]) -> bool {
    let Some(returned) = ret.named_child(0) else {
        return false;
    };
    if returned.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = returned.child_by_field_name("function") else {
        return false;
    };
    let Ok(text) = callee.utf8_text(source) else {
        return false;
    };
    text.rsplit("::").next().unwrap_or(text).trim() == "Err"
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
            // Negative space.
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
    fn is_under_tests_dir_matches_segments_and_file_names_exactly() {
        use std::path::Path;
        let cases = [
            // Existing `tests/` behavior, any depth.
            ("tests/helpers.rs", true),
            ("crates/foo/tests/it.rs", true),
            // New test-infrastructure segments.
            ("crates/foo/src/types/property_tests/gen.rs", true),
            ("crates/foo/src/test_utils/db.rs", true),
            ("crates/foo/src/test_helpers/mod.rs", true),
            ("crates/foo/src/testing/mod.rs", true),
            // New exact file names (cross-crate test helpers, no #[cfg(test)]).
            ("crates/foo/src/testing.rs", true),
            ("crates/foo/src/test_utils.rs", true),
            ("crates/foo/src/test_helpers.rs", true),
            // Negative space: non-exact segments / file names are production.
            ("crates/foo/src/lib.rs", false),
            ("crates/foo/src/my_testing.rs", false),
            ("crates/foo/src/testingground/k.rs", false),
            ("crates/foo/src/property_tests_old/gen.rs", false),
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
}
