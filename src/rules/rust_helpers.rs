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
}
