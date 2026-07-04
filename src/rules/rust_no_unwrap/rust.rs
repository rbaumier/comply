//! rust-no-unwrap backend.
//!
//! Flags `.unwrap()` and `.expect(...)` method calls in non-test code.
//! These turn runtime conditions (None / Err) into panics, which is the
//! opposite of what production code should do. Prefer `?` + proper error
//! types, or `unwrap_or_else` with a meaningful fallback.
//!
//! Tests are exempted — `.unwrap()` in a unit test is idiomatic because
//! a panic cleanly fails the test. We skip any call whose enclosing
//! function has `#[test]` or whose enclosing module has `#[cfg(test)]`.
//!
//! Testing-infrastructure crates are exempted — a crate declaring
//! `categories = ["development-tools::testing"]` (e.g. `tracing-mock`) is
//! dedicated test support where `.unwrap()` panicking on a failed expectation
//! is idiomatic, often from trait callbacks returning `()` where `?` cannot
//! propagate. The standardized crates.io category is an author-declared marker
//! of that purpose.
//!
//! build.rs is exempted — panics in Cargo build scripts are an acceptable
//! error mode during compilation (e.g. env::var("FOO").unwrap()).
//!
//! proc-macro crates are exempted — in a crate with `[lib] proc-macro = true`,
//! `.unwrap()`/`.expect()` runs while the downstream crate is compiled, so a
//! panic surfaces as a compile-time error, not a runtime abort. The rule's
//! "turns a runtime condition into a panic" rationale does not apply: there is
//! no runtime in a procedural macro.
//!
//! Example code is exempted — files under a Cargo `examples/` directory (or a
//! disabled variant like `examples_disabled/`) are illustrative, so `.unwrap()`
//! keeps them concise instead of obscuring the demonstrated feature with error
//! plumbing.
//!
//! Lock operations are exempted — `.read()`, `.write()`, `.lock()`,
//! `.try_lock()`, `.try_read()`, `.try_write()` followed by either `.unwrap()`
//! or `.expect(...)` are idiomatic for std::sync::{Mutex,RwLock} poisoning
//! propagation.
//!
//! Fixed-size-key delegation is exempted — `Self::new_from_slice(key).unwrap()`
//! where `key` is a parameter typed `&Key<…>` (a RustCrypto `GenericArray`
//! whose length is fixed by the type) cannot fail the length check, so the
//! unwrap is infallible. This is the prescribed `KeyInit::new` implementation
//! shape across `RustCrypto/block-ciphers`. The arg must be a `&Key<…>`-typed
//! parameter; `new_from_slice` on an arbitrary `&[u8]` still flags.
//!
//! Checked-arithmetic `.unwrap()` is exempted — `a.checked_mul(b).unwrap()`,
//! `a.checked_add(b).unwrap()`, etc. The `checked_*` integer methods return
//! `None` only when the operation has no valid result (overflow, and for
//! `checked_div`/`checked_rem` a zero divisor), so unwrapping them is a
//! deliberate "this can't overflow" assertion — strictly safer than the plain
//! `a * b` / `a + b` operator (which the rule does not flag and which silently
//! wraps in release builds). The `None` being unwrapped is an "impossible by
//! invariant" condition, the legitimate use of a panic, not careless
//! error-swallowing of a fallible Result/Option. Only `.unwrap()` on a
//! `checked_<arith>` receiver is exempt.
//!
//! Constant-bounds `try_into().unwrap()` is exempted — `slice[0..4].try_into()`
//! converting a slice into a fixed-size array is infallible by construction when
//! the index is a range with a constant length (`a[LIT..LIT]` or `a[..LIT]`), so
//! the unwrap is unreachable. This is idiomatic for parsing byte slices into word
//! arrays. The exemption keys on the constant range length alone, so a degenerate
//! literal range (`a[5..2]`) is also suppressed; a dynamic-length receiver
//! (`chunk`, `a[i..i+4]`, `a[4..]`) still flags.
//!
//! `Index`/`IndexMut` impls are exempted — `fn index`/`fn index_mut` return
//! `&Self::Output` / `&mut Self::Output`, never a `Result`/`Option`, so they
//! cannot propagate an error. Panicking on missing elements is the documented
//! trait contract (as `Vec`/`HashMap`/`BTreeMap` indexing does), making
//! `.unwrap()`/`.expect()` the only valid implementation. Matches bare and
//! path-qualified `impl Index`/`impl ops::Index`/`impl std::ops::IndexMut`.
//!
//! `.expect("documented reason")` in a scope that cannot propagate via `?` is
//! exempted — when the nearest enclosing function/closure's return type does
//! not denote a `Result` (an elided return type, `Option<_>`, `bool`, a plain
//! value, etc.), `?` is syntactically impossible, so a documented `.expect()`
//! is the only non-API-breaking invariant assertion (the same reasoning the
//! `Index`/`IndexMut` exemption applies to `fn index`). The message must be a
//! non-empty string literal; `.expect("")` and `.unwrap()` — which carry no
//! documented reason — still flag, and any `Result` return (including
//! `io::Result`, `fmt::Result`, `anyhow::Result`) keeps flagging because `?`
//! works there. The walk bails (keeps flagging) at an `async_block` boundary,
//! where error propagation is undeterminable.
//!
//! `.expect(non_string_message)` is exempted — `Option::expect`/`Result::expect`
//! always take a `&str`/`String` message, so a `.expect(enum_variant)` /
//! `.expect(call())` is a different, same-named domain method (e.g. ruff's
//! `Parser::expect(TokenKind) -> bool`). `.expect()` is flagged only when its
//! first argument is a string-producing expression — a string/raw-string literal
//! or a `format!`/`format_args!`/`concat!` macro, optionally behind a leading
//! `&`. A bare identifier/const message is ambiguous between a `&str` message and
//! a domain value, so it is left unflagged, the false-positive-safe direction.
//!
//! This rule is equivalent to `clippy::unwrap_used` + `clippy::expect_used`
//! (both restriction-group lints, off by default in clippy). Running it
//! via comply means you get the check without having to enable the lints
//! in every consuming crate.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::path_utils::is_cargo_example_path;
use crate::rules::rust_helpers::{
    is_in_const_initializer, is_in_index_trait_impl, is_in_test_context, is_under_tests_dir,
};

const KINDS: &[&str] = &["call_expression"];

/// Integer `checked_<arith>` methods whose `.unwrap()` is a deliberate overflow
/// assertion (returns `None` only on overflow), not careless error handling.
const CHECKED_ARITH_METHODS: &[&str] = &[
    "checked_add",
    "checked_add_signed",
    "checked_add_unsigned",
    "checked_sub",
    "checked_sub_signed",
    "checked_sub_unsigned",
    "checked_mul",
    "checked_div",
    "checked_div_euclid",
    "checked_rem",
    "checked_rem_euclid",
    "checked_pow",
    "checked_neg",
    "checked_abs",
    "checked_isqrt",
    "checked_ilog",
    "checked_ilog2",
    "checked_ilog10",
    "checked_shl",
    "checked_shr",
    "checked_next_power_of_two",
    "checked_next_multiple_of",
];

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
        if ctx.path.file_name() == Some(std::ffi::OsStr::new("build.rs")) {
            return;
        }
        let source_bytes = ctx.source.as_bytes();
        // Looking for `receiver.unwrap()` / `receiver.expect("…")`.
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        if function.kind() != "field_expression" {
            return;
        }
        let Some(field) = function.child_by_field_name("field") else {
            return;
        };
        let Ok(field_text) = field.utf8_text(source_bytes) else {
            return;
        };
        if field_text != "unwrap" && field_text != "expect" {
            return;
        }
        // Skip test code — `.unwrap()` is fine there.
        if is_in_test_context(node, source_bytes) || is_under_tests_dir(ctx.path) {
            return;
        }
        // Skip crates declaring `categories = ["development-tools::testing"]` —
        // dedicated test infrastructure (e.g. `tracing-mock`) where `.unwrap()`
        // panicking on a failed expectation is idiomatic, often from trait
        // callbacks returning `()` where `?` cannot propagate. The standardized
        // crates.io category is an author-declared marker of that purpose.
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.is_testing_crate())
        {
            return;
        }
        // Skip proc-macro crates — in a `[lib] proc-macro = true` crate,
        // `.unwrap()`/`.expect()` runs while the downstream crate compiles, so a
        // panic is a compile-time error, not a runtime abort. The rule's runtime
        // rationale does not hold there.
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.is_proc_macro())
        {
            return;
        }
        // Skip example code — `.unwrap()` keeps examples concise.
        if is_cargo_example_path(ctx.path) {
            return;
        }
        // Skip const/static item initializers — `unwrap`/`expect` is const-evaluated
        // at compile time and is the only valid way to extract the value there.
        if is_in_const_initializer(node) {
            return;
        }
        // Skip `Index`/`IndexMut` impl bodies — `fn index`/`fn index_mut` return a
        // reference, never a `Result`/`Option`, so panicking on a missing element
        // is the documented trait contract and `unwrap`/`expect` is the only valid
        // implementation.
        if is_in_index_trait_impl(node, source_bytes) {
            return;
        }
        // Skip lock operations and constant-bounds `try_into()` — both call the
        // unwrap/expect on the result of an inner `recv.METHOD()` call.
        if let Some((method, inner_func)) =
            inner_method_call(function.child_by_field_name("value"), source_bytes)
        {
            // .read()/.write()/.lock()/.try_lock() unwrap/expect is idiomatic
            // for std::sync::{Mutex,RwLock} poisoning propagation.
            if (field_text == "unwrap" || field_text == "expect")
                && matches!(
                    method,
                    "read" | "write" | "lock" | "try_lock" | "try_read" | "try_write"
                )
            {
                return;
            }
            // `a.checked_mul(b).unwrap()` / `a.checked_add(b).unwrap()` etc.:
            // the `checked_*` integer methods return `None` only on overflow, so
            // the unwrap is a deliberate overflow assertion, not careless error
            // handling — and strictly safer than the unchecked `*`/`+` operator.
            if field_text == "unwrap" && CHECKED_ARITH_METHODS.contains(&method) {
                return;
            }
            // `slice[LIT..LIT].try_into().unwrap()` parsing a fixed-length byte
            // slice into a same-sized array: the constant range length makes the
            // conversion infallible by construction.
            if method == "try_into"
                && is_constant_bounds_slice_index(inner_func.child_by_field_name("value"))
            {
                return;
            }
        }
        // Skip RustCrypto `KeyInit::new` delegation — `new_from_slice(key).unwrap()`
        // where `key` is a `&Key<…>`-typed parameter cannot fail the length check.
        if field_text == "unwrap" && is_fixed_size_key_delegation(function, node, source_bytes) {
            return;
        }
        // `Option::expect`/`Result::expect` always take a string MESSAGE argument.
        // A `.expect(non_string)` — e.g. a domain `Parser::expect(TokenKind::X) ->
        // bool` — only shares the method name and is a different method entirely,
        // so flag `.expect()` only when its first argument is a string-producing
        // expression (a string/raw-string literal, or a `format!`/`format_args!`/
        // `concat!` macro, optionally behind a leading `&`). A bare `identifier`
        // /const message argument is ambiguous between a `&str` message and a
        // domain value, so it is left unflagged — the false-positive-safe direction.
        if field_text == "expect" && !first_arg_is_string_message(node, source_bytes) {
            return;
        }
        // Skip `.expect("documented reason")` when the enclosing function/closure
        // cannot propagate via `?` — its return type does not denote a `Result`.
        // `?` is then syntactically impossible, so a documented `.expect()` is the
        // only non-API-breaking invariant assertion. `.unwrap()` (no message) and
        // `.expect("")` (no documented reason) are never exempted here.
        if field_text == "expect"
            && expect_has_nonempty_message(node)
            && enclosing_fn_cannot_propagate(node, source_bytes)
        {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-unwrap".into(),
            message: format!(
                "`.{field_text}()` turns a runtime condition into a panic. \
                 Use `?` with a proper error type, or `unwrap_or_else` with \
                 a meaningful fallback. Tests are exempted."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True when `call`'s first argument is a non-empty string literal — i.e. this
/// is `.expect("documented reason")`. `.expect("")` (empty message) and
/// `.expect(non_literal)` return false; a bare `.unwrap()` (no argument) never
/// reaches here.
fn expect_has_nonempty_message(call: tree_sitter::Node) -> bool {
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    let Some(first) = args.named_child(0) else {
        return false;
    };
    if !matches!(first.kind(), "string_literal" | "raw_string_literal") {
        return false;
    }
    // A non-empty string literal carries a `string_content` child spanning its
    // text; an empty `""` / `r""` has none.
    let mut cursor = first.walk();
    first
        .named_children(&mut cursor)
        .any(|c| c.kind() == "string_content" && !c.byte_range().is_empty())
}

/// True when `call`'s first argument is a string-producing MESSAGE expression: a
/// `string_literal` / `raw_string_literal`, or a `format!` / `format_args!` /
/// `concat!` macro invocation, after peeling an optional leading `&`
/// (`reference_expression`). `Option::expect`/`Result::expect` always take such a
/// `&str`/`String` message, so this separates a genuine panic-on-`None`/`Err`
/// from a domain `.expect(enum_variant)` / `.expect(non_string)` method that
/// merely shares the name. A bare `identifier` / `scoped_identifier` message
/// argument is ambiguous and returns false (left unflagged).
fn first_arg_is_string_message(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    let Some(mut first) = args.named_child(0) else {
        return false;
    };
    // Peel a leading `&` — `.expect(&format!("…"))`.
    if first.kind() == "reference_expression" {
        let Some(inner) = first.child_by_field_name("value") else {
            return false;
        };
        first = inner;
    }
    match first.kind() {
        "string_literal" | "raw_string_literal" => true,
        "macro_invocation" => first
            .child_by_field_name("macro")
            .and_then(|m| m.utf8_text(source).ok())
            .is_some_and(|name| matches!(name, "format" | "format_args" | "concat")),
        _ => false,
    }
}

/// True when the nearest enclosing function/closure of `node` has a return type
/// that does not denote a `Result`, so `?` cannot propagate an error out of this
/// scope. Walks up to the first `function_item` / `closure_expression`: an
/// absent return type (`-> ()` elided) counts as non-Result; a present one is
/// non-Result iff its text contains no `Result` token (so `io::Result<…>`,
/// `anyhow::Result<T>`, `fmt::Result` keep flagging). Reads the `return_type`
/// field only, never the whole signature, so a *parameter* typed `Result<…>`
/// does not prevent the exemption. Returns false — keep flagging — when an
/// `async_block` is crossed first (propagation across an async boundary is
/// undeterminable) or no fn/closure encloses the call.
fn enclosing_fn_cannot_propagate(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "function_item" | "closure_expression" => {
                let Some(ret) = parent.child_by_field_name("return_type") else {
                    return true;
                };
                return ret.utf8_text(source).is_ok_and(|t| !t.contains("Result"));
            }
            "async_block" => return false,
            _ => cur = parent,
        }
    }
    false
}

/// True for the RustCrypto `KeyInit::new` shape:
/// `<…>::new_from_slice(key).unwrap()` where `key` is a single identifier
/// argument bound to an enclosing-`fn` parameter typed `&Key<…>`.
///
/// `Key<…>` (a `GenericArray` of fixed length) makes `new_from_slice`'s length
/// check unreachable, so the unwrap is infallible. The argument must be such a
/// parameter — `new_from_slice` on an arbitrary `&[u8]` still flags.
///
/// `function` is the `field_expression` (`<call>.unwrap`); `unwrap_call` is the
/// enclosing `call_expression`.
fn is_fixed_size_key_delegation(
    function: tree_sitter::Node,
    unwrap_call: tree_sitter::Node,
    source: &[u8],
) -> bool {
    // Receiver must be `<…>::new_from_slice(<arg>)`.
    let Some(receiver) = function.child_by_field_name("value") else {
        return false;
    };
    if receiver.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = receiver.child_by_field_name("function") else {
        return false;
    };
    if !callee_named_new_from_slice(callee, source) {
        return false;
    }
    // Single identifier argument.
    let Some(arg) = sole_identifier_argument(receiver) else {
        return false;
    };
    // That identifier must be a parameter of the enclosing fn typed `&Key<…>`.
    enclosing_fn_has_key_typed_param(unwrap_call, arg, source)
}

/// True if the call target's final path segment is `new_from_slice` —
/// handles `Self::new_from_slice`, `Foo::new_from_slice`, and `x.new_from_slice`.
fn callee_named_new_from_slice(callee: tree_sitter::Node, source: &[u8]) -> bool {
    let name = match callee.kind() {
        "scoped_identifier" => callee.child_by_field_name("name"),
        "field_expression" => callee.child_by_field_name("field"),
        "identifier" => Some(callee),
        _ => None,
    };
    name.and_then(|n| n.utf8_text(source).ok()) == Some("new_from_slice")
}

/// Returns the sole argument of a `call_expression` when it is a bare
/// `identifier`, else `None`.
fn sole_identifier_argument(call: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let mut found: Option<tree_sitter::Node> = None;
    for child in args.named_children(&mut cursor) {
        if found.is_some() {
            return None; // more than one argument
        }
        found = Some(child);
    }
    let arg = found?;
    (arg.kind() == "identifier").then_some(arg)
}

/// True if the nearest enclosing `function_item` declares a parameter whose
/// name matches `arg`'s text and whose type is `&Key<…>` (a `reference_type`
/// wrapping a `generic_type` named `Key`).
fn enclosing_fn_has_key_typed_param(
    from: tree_sitter::Node,
    arg: tree_sitter::Node,
    source: &[u8],
) -> bool {
    let Ok(arg_name) = arg.utf8_text(source) else {
        return false;
    };
    let mut cur = from;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item" {
            let Some(params) = parent.child_by_field_name("parameters") else {
                return false;
            };
            let mut cursor = params.walk();
            for param in params.named_children(&mut cursor) {
                if param.kind() != "parameter" {
                    continue;
                }
                let Some(pattern) = param.child_by_field_name("pattern") else {
                    continue;
                };
                if pattern.utf8_text(source).ok() != Some(arg_name) {
                    continue;
                }
                return param
                    .child_by_field_name("type")
                    .is_some_and(|ty| is_reference_to_key_generic(ty, source));
            }
            return false;
        }
        cur = parent;
    }
    false
}

/// True for a `reference_type` whose inner type is `Key<…>` (a `generic_type`
/// with a `type_identifier` base of `Key`).
fn is_reference_to_key_generic(ty: tree_sitter::Node, source: &[u8]) -> bool {
    if ty.kind() != "reference_type" {
        return false;
    }
    let Some(inner) = ty.child_by_field_name("type") else {
        return false;
    };
    if inner.kind() != "generic_type" {
        return false;
    }
    inner
        .child_by_field_name("type")
        .and_then(|base| base.utf8_text(source).ok())
        == Some("Key")
}

/// For a `recv.METHOD(...)` receiver, returns `(METHOD, field_expression)` where
/// the returned node is the `recv.METHOD` field access (its `value` field is
/// `recv`). Returns `None` when the receiver is not a method call.
fn inner_method_call<'a>(
    receiver: Option<tree_sitter::Node<'a>>,
    source_bytes: &'a [u8],
) -> Option<(&'a str, tree_sitter::Node<'a>)> {
    let recv = receiver?;
    if recv.kind() != "call_expression" {
        return None;
    }
    let inner_func = recv.child_by_field_name("function")?;
    if inner_func.kind() != "field_expression" {
        return None;
    }
    let method = inner_func.child_by_field_name("field")?.utf8_text(source_bytes).ok()?;
    Some((method, inner_func))
}

/// True when `node` is `expr[RANGE]` whose range has a compile-time-constant
/// length: `expr[LIT..LIT]` / `expr[LIT..=LIT]` (two literals) or `expr[..LIT]`
/// (one literal, lower bound implicitly 0). Such an index yields a slice whose
/// length is fixed, making a following `try_into()` into a same-sized array
/// infallible. Open-ended (`expr[LIT..]`, `expr[..]`), variable-bound, or
/// non-index receivers return false — their length is not known at compile time.
fn is_constant_bounds_slice_index(node: Option<tree_sitter::Node>) -> bool {
    let Some(node) = node else {
        return false;
    };
    if node.kind() != "index_expression" {
        return false;
    }
    // index_expression named children are [receiver, index]; the index is last.
    let Some(index) = node.named_child(node.named_child_count().saturating_sub(1)) else {
        return false;
    };
    if index.kind() != "range_expression" {
        return false;
    }
    // The range must carry a constant length: `LIT..LIT` (two literals) or
    // `..LIT` (one literal, lower bound implicitly 0). `LIT..` / `..` are
    // open-ended and depend on the source length, so they are not constant.
    let mut child = index.walk();
    let bounds: Vec<tree_sitter::Node> = index.named_children(&mut child).collect();
    let is_int_lit = |n: tree_sitter::Node| -> bool { n.kind() == "integer_literal" };
    match bounds.len() {
        // `LIT..LIT`
        2 => is_int_lit(bounds[0]) && is_int_lit(bounds[1]),
        // `..LIT` only — distinguish from `LIT..` by the leading `..` token.
        1 => is_int_lit(bounds[0]) && starts_with_dotdot(index),
        _ => false,
    }
}

/// True when the range's first token is `..`, i.e. it has no lower bound
/// (`..LIT`). Used to accept `..LIT` while rejecting `LIT..` (open upper bound),
/// which share a single named child.
fn starts_with_dotdot(range: tree_sitter::Node) -> bool {
    range.child(0).is_some_and(|first| first.kind() == "..")
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

    /// Run on a file next to the given `Cargo.toml` so the manifest
    /// (`development-tools::testing` category exemption) resolves via
    /// `nearest_cargo_manifest`.
    fn run_on_with_cargo(cargo_toml_contents: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_cargo(
            &Check,
            cargo_toml_contents,
            source,
            "src/expect.rs",
        )
    }

    const TESTING_CARGO_TOML: &str = r#"
[package]
name = "tracing-mock"
version = "0.1.0"
edition = "2021"
categories = ["development-tools::testing"]
"#;

    const LIB_CARGO_TOML: &str = r#"
[package]
name = "normal-lib"
version = "0.1.0"
edition = "2021"

[lib]
name = "normal_lib"
"#;

    const PROC_MACRO_CARGO_TOML: &str = r#"
[package]
name = "derive-impl-like"
version = "0.1.0"
edition = "2021"

[lib]
proc-macro = true
"#;

    #[test]
    fn flags_unwrap_in_production_fn() {
        assert_eq!(run_on("fn f() { let x = y.unwrap(); }").len(), 1);
    }

    #[test]
    fn flags_expect_in_result_returning_fn() {
        // The enclosing fn returns `Result`, so `?` IS possible — `.expect()` is
        // flagged (the non-Result-return carve-out does not apply).
        assert_eq!(
            run_on(r#"fn f() -> Result<(), ()> { let x = y.expect("msg"); Ok(()) }"#).len(),
            1
        );
    }

    #[test]
    fn allows_unwrap_in_test_function() {
        let source = "#[test]\nfn it_works() { let x = y.unwrap(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unwrap_inside_cfg_test_module() {
        let source = "#[cfg(test)]\nmod tests { fn f() { let x = y.unwrap(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_question_mark() {
        assert!(run_on("fn f() -> Result<(), ()> { let x = y?; Ok(()) }").is_empty());
    }

    #[test]
    fn allows_unwrap_in_build_rs() {
        let source = r#"fn main() { let v = std::env::var("TARGET").unwrap(); }"#;
        assert!(crate::rules::test_helpers::run_rule(&Check, source, "build.rs").is_empty());
    }

    #[test]
    fn allows_unwrap_on_rwlock_read() {
        let source = "fn f(data: &RwLock<Vec<u8>>) -> Vec<u8> { data.read().unwrap().clone() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unwrap_on_mutex_lock() {
        let source = "fn f(m: &Mutex<u32>) -> u32 { *m.lock().unwrap() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_non_lock_unwrap() {
        assert_eq!(run_on("fn f() { let x = y.unwrap(); }").len(), 1);
    }

    #[test]
    fn allows_expect_on_mutex_lock() {
        // #7143: `.lock().expect("reason")` is the same mutex-poisoning idiom as
        // the already-exempt `.lock().unwrap()`. The fn returns `Result`, so the
        // non-Result-return `.expect()` carve-out does not apply — only the lock
        // exemption keeps this from flagging.
        let source =
            r#"fn f(m: &Mutex<u32>) -> Result<u32, E> { Ok(*m.lock().expect("lock should succeed")) }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_expect_on_rwlock_read() {
        let source = r#"fn f(l: &RwLock<u8>) -> Result<u8, E> { Ok(*l.read().expect("read")) }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_expect_on_rwlock_write() {
        let source =
            r#"fn f(l: &RwLock<u8>) -> Result<(), E> { *l.write().expect("write") = 1; Ok(()) }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_expect_on_try_lock() {
        let source =
            r#"fn f(m: &Mutex<u8>) -> Result<u8, E> { Ok(*m.try_lock().expect("try_lock")) }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_expect_on_non_lock_method_in_result_fn() {
        // #7143 scope guard: broadening the outer call to `.expect()` applies ONLY
        // when the inner receiver is a lock method. A non-lock `.foo().expect()` in
        // a Result-returning fn (where `?` is possible) still flags.
        let source = r#"fn f(x: &X) -> Result<u8, E> { Ok(x.foo().expect("nope")) }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_unwrap_in_tests_directory() {
        let source = "pub fn helper() { let x = y.unwrap(); }";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, source, "tests/helpers.rs").is_empty()
        );
    }

    #[test]
    fn allows_unwrap_in_testing_rs() {
        let source = "pub fn h() { let x = y.unwrap(); }";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, source, "crates/foo/src/testing.rs")
                .is_empty()
        );
    }

    #[test]
    fn allows_unwrap_in_test_utils_rs() {
        let source = "pub fn h() { let x = y.unwrap(); }";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, source, "crates/foo/src/test_utils.rs")
                .is_empty()
        );
    }

    #[test]
    fn allows_unwrap_in_testutil_rs() {
        // ripgrep's crates/searcher/src/testutil.rs — the FP from #3282.
        let source = "pub fn h() { let x = y.unwrap(); }";
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                source,
                "crates/searcher/src/testutil.rs"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_unwrap_under_testutil_dir() {
        let source = "pub fn h() { let x = y.unwrap(); }";
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                source,
                "crates/foo/src/testutil/mod.rs"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_unwrap_under_property_tests_dir() {
        let source = "pub fn gen() { let x = y.unwrap(); }";
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                source,
                "crates/foo/src/types/property_tests/gen.rs"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_unwrap_in_name_tests_file() {
        // zellij-org/zellij: `.unwrap()` in a co-located `_tests.rs` unit file
        // is idiomatic test code (#7121).
        let source = "fn t() { let x = y.unwrap(); }";
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                source,
                "zellij-server/src/panes/tiled_panes/unit/stacked_panes_tests.rs"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_unwrap_in_normal_source_sibling() {
        // A production source file next to a `_tests.rs` sibling still flags:
        // its stem carries no `test`/`tests` token.
        let source = "pub fn z() { let x = y.unwrap(); }";
        assert_eq!(
            crate::rules::test_helpers::run_rule(
                &Check,
                source,
                "zellij-server/src/panes/tiled_panes.rs"
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_unwrap_in_ordinary_src_file() {
        let source = "pub fn z() { let x = y.unwrap(); }";
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, source, "crates/foo/src/lib.rs").len(),
            1
        );
    }

    #[test]
    fn flags_unwrap_in_non_exact_testing_name() {
        // `my_testing.rs` is not an exact match for `testing.rs`.
        let source = "pub fn m() { let x = y.unwrap(); }";
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, source, "crates/foo/src/my_testing.rs")
                .len(),
            1
        );
    }

    #[test]
    fn flags_unwrap_in_non_exact_testing_dir() {
        // `testingground/` is not an exact match for `testing`.
        let source = "pub fn tg() { let x = y.unwrap(); }";
        assert_eq!(
            crate::rules::test_helpers::run_rule(
                &Check,
                source,
                "crates/foo/src/testingground/k.rs"
            )
            .len(),
            1
        );
    }

    #[test]
    fn allows_unwrap_in_examples_dir() {
        let source = "fn main() { let x = y.unwrap(); }";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, source, "examples/migration/src/main.rs")
                .is_empty()
        );
    }

    #[test]
    fn allows_unwrap_in_examples_disabled_dir() {
        // #4779: fjall keeps disabled examples in `examples_disabled/` — still
        // illustrative example code where `.unwrap()` is idiomatic.
        let source = "fn main() { let val = tree.get(b\"user#0\").unwrap().unwrap(); }";
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                source,
                "examples_disabled/migration/src/main.rs"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_unwrap_in_production_src() {
        // A genuine production `src/` file still flags.
        let source = "pub fn run() { let x = y.unwrap(); }";
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, source, "src/lib.rs").len(),
            1
        );
    }

    #[test]
    fn allows_unwrap_in_const_item_initializer() {
        // #3860: `NonZeroU32::new(_).unwrap()` is the canonical way to build a
        // const value — `?` does not compile and `unwrap_or_else` is not const.
        let source = "impl W { pub const ONE: W = W(NonZeroU32::new(1).unwrap()); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unwrap_in_static_item_initializer() {
        let source = "static S: u32 = foo().unwrap();";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unwrap_on_new_from_slice_with_key_param() {
        // #4843: RustCrypto `KeyInit::new` delegates to `new_from_slice(key)`
        // where `key: &Key<Self>` is a fixed-size GenericArray — the length
        // check is unreachable, so the unwrap is infallible.
        let source = r#"impl KeyInit for Xtea {
    fn new(key: &Key<Self>) -> Self {
        Self::new_from_slice(key).unwrap()
    }
}"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_unwrap_on_new_from_slice_with_byte_slice_param() {
        // `new_from_slice` on an arbitrary `&[u8]` can fail the length check —
        // the unwrap is a real panic risk and must still flag.
        let source = r#"fn build(key: &[u8]) -> Self {
    Self::new_from_slice(key).unwrap()
}"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unwrap_on_new_from_slice_with_non_param_arg() {
        // The argument is a local, not a `&Key<…>` parameter — still flags.
        let source = r#"fn build() -> Self {
    let key = read_key();
    Self::new_from_slice(&key).unwrap()
}"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unwrap_on_other_method_with_key_param() {
        // Only `new_from_slice` carries the length guarantee; an unrelated
        // fallible call on a `&Key<…>` param still flags.
        let source = r#"impl KeyInit for Xtea {
    fn new(key: &Key<Self>) -> Self {
        Self::try_from(key).unwrap()
    }
}"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_try_into_unwrap_on_constant_range_index() {
        // #4840: `key[0..4].try_into().unwrap()` converting a fixed-length slice
        // into a same-sized array cannot fail — the unwrap is unreachable.
        let source =
            "fn f(key: &[u8]) -> u32 { u32::from_le_bytes(key[0..4].try_into().unwrap()) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_try_into_expect_on_constant_range_index() {
        let source = r#"fn f(b: &[u8]) -> u16 { u16::from_le_bytes(b[2..4].try_into().expect("4 bytes")) }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_try_into_unwrap_on_open_lower_bound_index() {
        // `..4` has a constant length (lower bound is implicitly 0).
        let source = "fn f(b: &[u8]) -> u32 { u32::from_le_bytes(b[..4].try_into().unwrap()) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_try_into_unwrap_on_inclusive_range_index() {
        // `0..=3` has a constant length (4), so the conversion is infallible.
        let source = "fn f(b: &[u8]) -> u32 { u32::from_le_bytes(b[0..=3].try_into().unwrap()) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_try_into_unwrap_on_open_upper_bound_index() {
        // `b[4..]` is open-ended — its length depends on the source, so the
        // conversion is fallible and the unwrap must still flag.
        let source = "fn f(b: &[u8]) -> [u8; 2] { b[4..].try_into().unwrap() }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_try_into_unwrap_on_variable_bound_index() {
        // `key[i..i+4]` has a dynamic bound — not constant, still flags.
        let source = "fn f(key: &[u8], i: usize) -> u32 { u32::from_le_bytes(key[i..i+4].try_into().unwrap()) }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_try_into_unwrap_on_plain_slice() {
        // No index expression at all — receiver is a bare identifier; fallible.
        let source = "fn f(chunk: &[u8]) -> u32 { u32::from_le_bytes(chunk.try_into().unwrap()) }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_try_into_unwrap_on_full_range_index() {
        // `b[..]` reborrows the whole slice — length unknown, still flags.
        let source = "fn f(b: &[u8]) -> [u8; 2] { b[..].try_into().unwrap() }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unwrap_in_const_fn_body() {
        // A `const fn` body is a runtime body that can return `Result` / use `?`,
        // so unwrap there is still flagged.
        let source = "const fn f(x: Option<u32>) -> u32 { x.unwrap() }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_expect_in_path_qualified_index_impl() {
        // #4919: toml_edit's `impl ops::Index<&str> for Table` — `fn index`
        // returns `&Item`, so panicking on a missing key is the trait contract.
        let source = r#"impl<'s> ops::Index<&'s str> for Table {
    type Output = Item;
    fn index(&self, key: &'s str) -> &Item {
        self.get(key).expect("index not found")
    }
}"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unwrap_in_bare_index_mut_impl() {
        // Bare `impl IndexMut<…> for T` (no `ops::` path) and `.unwrap()`.
        let source = r#"impl IndexMut<usize> for Grid {
    fn index_mut(&mut self, i: usize) -> &mut Cell {
        self.cells.get_mut(i).unwrap()
    }
}"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_unwrap_in_non_index_trait_impl() {
        // A non-Index trait whose method *can* return `Result` — the unwrap is a
        // real panic risk and must still flag.
        let source = r#"impl Loader for Db {
    fn load(&self, k: &str) -> Result<Value, Error> {
        Ok(self.get(k).unwrap())
    }
}"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_unwrap_on_checked_mul() {
        // #5934: `data.len().checked_mul(8).unwrap()` for a `Vec::with_capacity`
        // is a deliberate overflow assertion, not careless error handling.
        let source =
            "fn f(data: &[u8]) -> usize { data.len().checked_mul(8).unwrap() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unwrap_on_checked_add() {
        // #5934: chained `checked_mul(...).unwrap().checked_add(...).unwrap()`.
        let source = "fn f(text: &str) -> usize { text.len().checked_mul(3).unwrap().checked_add(text.len().div_ceil(3)).unwrap() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_unwrap_on_parse_result() {
        // A careless unwrap on a genuinely fallible parse still flags.
        let source = r#"fn f(s: &str) -> u32 { s.parse::<u32>().unwrap() }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unwrap_on_file_open() {
        // A careless unwrap on a fallible I/O call still flags.
        let source = r#"fn f(p: &str) -> File { File::open(p).unwrap() }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unwrap_on_map_get() {
        // A careless unwrap on an `Option` from a map lookup still flags.
        let source = "fn f(map: &HashMap<u32, u32>, k: u32) -> u32 { *map.get(&k).unwrap() }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_expect_on_checked_mul() {
        // `.expect()` is itself flagged; the checked-arith exemption is
        // `.unwrap()`-only, so `checked_mul(...).expect(...)` still flags. The fn
        // returns `Result`, so `?` is possible and the non-Result-return carve-out
        // does not apply.
        let source =
            r#"fn f(a: usize, b: usize) -> Result<usize, E> { Ok(a.checked_mul(b).expect("overflow")) }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unwrap_in_inherent_index_fn() {
        // An inherent `impl T` with a method merely *named* `index` has no `trait`
        // field, so the exemption must not apply — the unwrap still flags.
        let source = r#"impl Grid {
    fn index(&self, i: usize) -> &Cell {
        self.cells.get(i).unwrap()
    }
}"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_expect_in_option_returning_fn() {
        // #6251: `fn search(&self) -> Option<Match>` cannot propagate via `?`, so
        // a documented `.expect()` is the only non-API-breaking invariant assertion.
        let source = r#"fn search(&self) -> Option<Match> { self.aut.try_find(&self.input).expect("already checked") }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_expect_in_bool_returning_fn() {
        // #6251: `fn is_match(&self) -> bool` — same reasoning, `?` impossible.
        let source = r#"fn is_match(&self) -> bool { self.aut.try_find(&i).expect("not expected to fail").is_some() }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_expect_in_unit_returning_fn() {
        // #6251: no return type → the fn returns `()` → `?` impossible → exempt.
        let source = r#"fn f() { let x = y.expect("invariant"); }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_unwrap_in_option_returning_fn() {
        // #6251: `.unwrap()` carries no documented reason — it is never exempted by
        // the non-Result-return carve-out, even where `?` is impossible.
        let source =
            r#"fn search(&self) -> Option<Match> { self.aut.try_find(&self.input).unwrap() }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_expect_in_io_result_fn() {
        // #6251: a path-qualified `io::Result` return still contains `Result`, so
        // `?` is possible and `.expect()` keeps flagging.
        let source = r#"fn f() -> io::Result<()> { let x = y.expect("m"); Ok(()) }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_empty_expect_in_non_result_fn() {
        // #6251: `.expect("")` has no documented reason, so it still flags even
        // where `?` is impossible.
        let source = r#"fn f() -> bool { y.expect("") }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_expect_in_async_block_in_non_result_fn() {
        // #6251: error propagation across an async boundary is undeterminable, so
        // the walk bails at `async_block` and keeps flagging even though the
        // enclosing fn returns a non-Result type.
        let source = r#"fn spawn(&self) -> Handle { run(async move { x.expect("m") }) }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_expect_in_non_result_closure() {
        // #6251: a closure with an explicit non-Result return type cannot propagate
        // via `?`, so a documented `.expect()` is exempt at the closure boundary.
        let source = r#"fn f(v: &[Item]) { v.iter().map(|x| -> u8 { x.expect("m") }); }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_expect_with_result_typed_param_in_non_result_fn() {
        // #6251: the `return_type` field is read, not the signature, so a
        // *parameter* typed `Result<…>` does not block the exemption.
        let source = r#"fn f(x: Result<u8, E>) -> bool { x.expect("m") }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_expect_with_enum_variant_arg() {
        // #7159: ruff's `Parser::expect(&mut self, expected: TokenKind) -> bool`
        // shares the name with `Option/Result::expect` but takes an enum-variant
        // argument, not a string message — a different method that must not flag.
        let source = r#"fn parse(&mut self) -> bool { self.expect(TokenKind::Import) }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_expect_with_scoped_enum_arg_in_result_fn() {
        // #7159: a non-string arg is exempt even in a `Result`-returning fn (where
        // `?` works), because the receiver is a domain type, not an `Option`/`Result`.
        let source = r#"fn f(cx: &Ctx) -> Result<(), E> { Ok(cx.expect(SomeEnum::V)) }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_expect_with_call_arg() {
        // #7159: `.expect(some_call())` — a call-expression arg is not a string message.
        let source = r#"fn f(parser: &Parser) -> bool { parser.expect(next_token()) }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_expect_with_field_arg() {
        // #7159: `.expect(self.field)` — a field-expression arg is not a string message.
        let source = r#"fn f(&self) -> bool { self.expect(self.kind) }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_expect_with_bare_identifier_arg() {
        // #7159: accepted trade-off — a bare identifier message (`opt.expect(msg)`)
        // is ambiguous between a `&str` message and a domain value, so it is
        // exempted, the false-positive-safe direction.
        let source =
            r#"fn f(opt: Option<u8>, msg: &str) -> Result<u8, E> { Ok(opt.expect(msg)) }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_expect_with_string_literal_in_result_fn() {
        // #7159: a genuine `Option/Result::expect("reason")` — string-literal
        // message — still flags where `?` is possible.
        let source = r#"fn f(opt: Option<u8>) -> Result<u8, E> { Ok(opt.expect("reason")) }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_expect_with_format_message_in_result_fn() {
        // #7159: `opt.expect(format!("…"))` produces a `String` message — a genuine
        // `expect` that must still flag (guards against a string-literal-only false
        // negative).
        let source = r#"fn f(opt: Option<u8>, x: u8) -> Result<u8, E> { Ok(opt.expect(format!("failed {x}"))) }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_expect_with_ref_format_message_in_result_fn() {
        // #7159: `opt.expect(&format!("…"))` — the leading `&` is peeled; still a
        // genuine string message that must flag.
        let source = r#"fn f(opt: Option<u8>, x: u8) -> Result<u8, E> { Ok(opt.expect(&format!("failed {x}"))) }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// Closes #7014: tracing-mock's `Cargo.toml` declares
    /// `categories = ["development-tools::testing"]`, so `.unwrap()`/`.expect()`
    /// in its normally-named module files (`expect.rs`) must not flag.
    #[test]
    fn allows_unwrap_in_testing_category_crate() {
        assert!(
            run_on_with_cargo(TESTING_CARGO_TOML, "pub fn h() { let x = y.unwrap(); }").is_empty(),
            ".unwrap() in a development-tools::testing crate must not flag"
        );
        assert!(
            run_on_with_cargo(TESTING_CARGO_TOML, r#"pub fn h() { let x = y.expect("nope"); }"#)
                .is_empty(),
            ".expect() in a development-tools::testing crate must not flag"
        );
    }

    /// Negative space: an ordinary library crate without the testing category
    /// must keep flagging `.unwrap()` even in a normally-named module file.
    #[test]
    fn still_flags_unwrap_in_non_testing_category_crate() {
        assert_eq!(
            run_on_with_cargo(LIB_CARGO_TOML, "pub fn h() { let x = y.unwrap(); }").len(),
            1,
            ".unwrap() in a crate without the testing category must still flag"
        );
    }

    /// The manifest predicate the exemption keys on: a `development-tools::testing`
    /// category parses to `is_testing_crate()`; a plain `[lib]` table does not.
    #[test]
    fn manifest_detects_testing_crate() {
        use crate::project::CargoManifest;
        use std::path::PathBuf;
        let testing = CargoManifest::parse(TESTING_CARGO_TOML, PathBuf::from("/c")).unwrap();
        assert!(testing.is_testing_crate());
        let normal = CargoManifest::parse(LIB_CARGO_TOML, PathBuf::from("/c")).unwrap();
        assert!(!normal.is_testing_crate());
    }

    /// Closes #7158: astral-sh/ruff's `crates/ruff_macros` declares
    /// `[lib] proc-macro = true`. A proc-macro's `.unwrap()`/`.expect()` panics
    /// at compile time (there is no runtime), so neither must flag.
    #[test]
    fn allows_unwrap_in_proc_macro_crate() {
        assert!(
            run_on_with_cargo(PROC_MACRO_CARGO_TOML, "pub fn h() { let x = y.unwrap(); }")
                .is_empty(),
            ".unwrap() in a proc-macro crate must not flag"
        );
        assert!(
            run_on_with_cargo(
                PROC_MACRO_CARGO_TOML,
                r#"pub fn h() { let x = y.expect("named fields"); }"#
            )
            .is_empty(),
            ".expect() in a proc-macro crate must not flag"
        );
    }

    /// The manifest predicate the proc-macro exemption keys on: `[lib]
    /// proc-macro = true` parses to `is_proc_macro()`; a plain `[lib]` table does
    /// not.
    #[test]
    fn manifest_detects_proc_macro_crate() {
        use crate::project::CargoManifest;
        use std::path::PathBuf;
        let proc_macro = CargoManifest::parse(PROC_MACRO_CARGO_TOML, PathBuf::from("/c")).unwrap();
        assert!(proc_macro.is_proc_macro());
        let normal = CargoManifest::parse(LIB_CARGO_TOML, PathBuf::from("/c")).unwrap();
        assert!(!normal.is_proc_macro());
    }
}
