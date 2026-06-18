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
///   `test_utils`, `test_helpers`, `testing`, or `testutil` — covers Cargo's
///   `tests/` integration directory, `property_tests/` generators, and shared
///   test-helper modules at any nesting depth; OR
/// - the file NAME is exactly `testing.rs`, `test_utils.rs`, `test_helpers.rs`,
///   or `testutil.rs`.
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
        "testutil",
    ];
    const TEST_FILE_NAMES: &[&str] =
        &["testing.rs", "test_utils.rs", "test_helpers.rs", "testutil.rs"];

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

/// True if `node` is covered by an `#[allow(<scope>::<lint>)]` or
/// `#[expect(<scope>::<lint>)]` attribute naming `lint`, applied to an enclosing
/// statement, expression, or item.
///
/// Walks up from `node` via `parent()`; at each ancestor it scans the preceding
/// `attribute_item` siblings (skipping interleaved comment siblings, traversing
/// past unrelated attributes such as `#[cfg(...)]`) for an `allow`/`expect`
/// attribute whose argument `token_tree` contains an `identifier` token equal to
/// `lint`. The walk stops at the enclosing `function_item` / `closure_expression`
/// / `source_file` boundary so an `#[allow]` on a *sibling* item far above does
/// not leak in.
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
/// bare `pub` modifier itself AND no enclosing module restricts it.
///
/// Effective visibility is the product of the item's own modifier and every
/// enclosing module's, so a bare-`pub` item buried in a non-public module (e.g.
/// `mod imp { pub fn … }`) is not part of the crate's public API. Combines
/// [`is_pub`] (the item's own modifier) with [`is_inside_non_public_module`]
/// (the enclosing chain).
///
/// Rules whose rationale is "this is part of the crate's public surface" gate on
/// this rather than bare [`is_pub`].
pub fn is_effectively_pub(item: Node, source: &[u8]) -> bool {
    is_pub(item, source) && !is_inside_non_public_module(item, source)
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

/// Resolve the declared type of a local binding named `name` that is visible at
/// `node`. Walks up each enclosing scope (`function_item`, `closure_expression`,
/// `block`, `source_file`) and, within it, finds the nearest `parameter` or
/// `let_declaration` *before* `node` whose pattern binds `name` and carries an
/// explicit `type` annotation, returning that type's source text (trimmed).
///
/// Only annotated bindings are resolved — an inferred `let x = ...;` yields
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
/// - a method `call_expression` whose method name follows the established
///   bool-returning convention: an `is_`/`has_` prefix, or exactly `contains`,
///   `starts_with`, or `ends_with` (covers `value.is_some() as u8`);
/// - a bare `identifier` whose local binding is annotated `bool` (`b as u8`).
///
/// The method-name set is a deliberately narrow heuristic; it must not be
/// broadened, since an arbitrary method may return any integer type.
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

fn operand_is_bool(node: Node, source: &[u8]) -> bool {
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
        "call_expression" => call_method_returns_bool(node, source),
        "identifier" => node
            .utf8_text(source)
            .ok()
            .and_then(|name| find_identifier_type(node, name, source))
            .is_some_and(|type_text| type_text == "bool"),
        _ => false,
    }
}

/// True if `call` is a method call (`<receiver>.method(...)`) whose method name
/// follows the bool-returning convention: an `is_`/`has_` prefix, or exactly
/// `contains` / `starts_with` / `ends_with`.
fn call_method_returns_bool(call: Node, source: &[u8]) -> bool {
    const BOOL_METHODS: &[&str] = &["contains", "starts_with", "ends_with"];

    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    function
        .child_by_field_name("field")
        .and_then(|field| field.utf8_text(source).ok())
        .is_some_and(|name| {
            name.starts_with("is_") || name.starts_with("has_") || BOOL_METHODS.contains(&name)
        })
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
        .any(|variant| variant_is_cfg_gated(variant, source))
}

/// True if `enum_variant` carries a preceding `#[cfg(...)]` / `#[cfg_attr(...)]`
/// attribute. Skips interleaved comment siblings and stops at the first
/// non-attribute, non-comment sibling.
fn variant_is_cfg_gated(variant: Node, source: &[u8]) -> bool {
    let mut sibling = variant.prev_named_sibling();
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
        for (src, expected) in cases {
            let tree = parse(src);
            let func = first_of_kind(tree.root_node(), "function_item")
                .expect("snippet should contain a function_item");
            assert_eq!(
                is_effectively_pub(func, src.as_bytes()),
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
            ("crates/foo/src/testutil/mod.rs", true),
            // New exact file names (cross-crate test helpers, no #[cfg(test)]).
            ("crates/foo/src/testing.rs", true),
            ("crates/foo/src/test_utils.rs", true),
            ("crates/foo/src/test_helpers.rs", true),
            ("crates/searcher/src/testutil.rs", true),
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
            // Identifier whose binding is annotated bool.
            ("fn f(b: bool) -> u8 { b as u8 }", true),
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
}
