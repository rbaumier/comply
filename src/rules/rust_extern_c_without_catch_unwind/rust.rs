//! rust-extern-c-without-catch-unwind backend.
//!
//! Walks `function_item` nodes whose [`fn_extern_abi`] is `"C"` — which covers
//! the bare `extern fn` spelling, since `"C"` is the ABI Rust defaults it to —
//! and flags the ones whose body can panic without going through
//! `catch_unwind`.
//!
//! `extern "C-unwind"` is exempt by construction: that ABI exists precisely to
//! make unwinding across the boundary defined, so a panic there is not a bug.
//! Any other ABI (`"Rust"`, `"system"`, …) is out of scope. Declarations inside
//! an `extern "C" { … }` block never reach the rule either: the grammar spells
//! them `function_signature_item`, a different kind with no body.
//!
//! A body counts as unable to panic when it contains no call, no macro
//! invocation, no indexing, and no arithmetic or shift operator — the four
//! shapes that carry every panic a function can raise on its own
//! (`unwrap`/`expect` and `assert!` are calls and macros; a slice index is an
//! `index_expression`; `+` and `<<` panic on overflow in debug). A body reduced
//! to a field read, a cast, a comparison or a constant is therefore left alone,
//! which keeps trivial C shims (`*const T` getters, `is_null` predicates) quiet.
//!
//! The `catch_unwind` guard is textual: any mention of `catch_unwind` in the
//! body exempts it, whatever the qualification
//! (`std::panic::catch_unwind`, `panic::catch_unwind`, a bare
//! `catch_unwind(…)` after a `use`) and whether the call wraps the whole body
//! or sits inside the `unsafe { … }` block that is the body. Matching the call
//! shape instead would report functions that do guard themselves, which is the
//! expensive kind of false positive here.
//!
//! Test code is exempt: a panicking `extern "C"` callback in a test is usually
//! the thing under test.
//!
//! Known limitation: a crate built with `panic = "abort"` aborts on every panic
//! anyway, so `catch_unwind` buys it nothing. That setting lives in the
//! workspace root's `[profile.*]`, which the rule does not read, so such a crate
//! is flagged.

use tree_sitter::Node;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{fn_extern_abi, is_in_test_context, is_under_tests_dir};

/// Binary operators that panic on overflow or on a zero divisor. The comparison
/// and bitwise operators sharing the `binary_expression` kind cannot panic, so
/// they leave a body trivial.
const PANICKING_OPERATORS: &[&str] = &["+", "-", "*", "/", "%", "<<", ">>"];

/// Their compound-assignment twins, which panic on exactly the same inputs.
const PANICKING_ASSIGN_OPERATORS: &[&str] = &["+=", "-=", "*=", "/=", "%=", "<<=", ">>="];

crate::ast_check! { on ["function_item"] prefilter = ["extern"] => |node, source, ctx, diagnostics|
    // `"C-unwind"` defines unwinding across the boundary; every other ABI is
    // outside this rule's premise.
    if fn_extern_abi(node, source) != Some("C") { return; }
    if is_in_test_context(node, source) || is_under_tests_dir(ctx.path) { return; }

    let Some(body) = node.child_by_field_name("body") else { return; };
    let Ok(body_text) = body.utf8_text(source) else { return; };
    if body_text.contains("catch_unwind") { return; }
    if !subtree_can_panic(body, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "a panic escaping this `extern \"C\"` function aborts the process — the C caller \
         never regains control. Wrap the body in `std::panic::catch_unwind(|| { … })` and \
         return an error code, or declare it `extern \"C-unwind\"`."
            .to_string(),
        Severity::Error,
    ));
}

/// True when `node`'s subtree holds an operation that can panic: a call, a macro
/// invocation, an indexing, or an arithmetic/shift operator.
fn subtree_can_panic(node: Node, source: &[u8]) -> bool {
    if node_can_panic(node, source) {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| subtree_can_panic(child, source))
}

/// True when a single node is one of the panic-carrying shapes. The operator of
/// a binary or compound assignment is an unnamed child, so it is read through
/// its field rather than by walking children.
fn node_can_panic(node: Node, source: &[u8]) -> bool {
    match node.kind() {
        "call_expression" | "macro_invocation" | "index_expression" => true,
        "binary_expression" => operator_is(node, source, PANICKING_OPERATORS),
        "compound_assignment_expr" => operator_is(node, source, PANICKING_ASSIGN_OPERATORS),
        _ => false,
    }
}

fn operator_is(node: Node, source: &[u8], operators: &[&str]) -> bool {
    node.child_by_field_name("operator")
        .and_then(|op| op.utf8_text(source).ok())
        .is_some_and(|op| operators.contains(&op))
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "src/ffi.rs")
    }

    #[test]
    fn flags_extern_c_fn_that_unwraps() {
        let source = "#[no_mangle]\npub extern \"C\" fn parse(p: *const u8) -> i32 { \
             let s = to_str(p).unwrap(); s.len() as i32 }";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_unsafe_extern_c_fn_that_indexes() {
        let source = "pub unsafe extern \"C\" fn at(buf: &[u8], i: usize) -> u8 { buf[i] }";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_extern_c_fn_with_an_assert() {
        let source = "extern \"C\" fn check(n: i32) { assert!(n > 0); }";
        assert_eq!(run(source).len(), 1);
    }

    /// A bare `extern fn` is `extern "C" fn` — Rust supplies the default ABI.
    #[test]
    fn flags_bare_extern_fn_with_arithmetic() {
        let source = "extern fn add(a: i32, b: i32) -> i32 { a + b }";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_extern_c_unwind_fn() {
        let source = "pub extern \"C-unwind\" fn run(cb: fn()) { cb(); }";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_extern_c_fn_wrapped_in_catch_unwind() {
        let source = "#[no_mangle]\npub extern \"C\" fn parse(p: *const u8) -> i32 { \
             std::panic::catch_unwind(|| to_str(p).unwrap().len() as i32).unwrap_or(-1) }";
        assert!(run(source).is_empty());
    }

    /// The guard may sit inside the `unsafe { … }` block that is the body.
    #[test]
    fn allows_catch_unwind_nested_in_an_unsafe_block() {
        let source = "pub unsafe extern \"C\" fn run(h: *mut Handle) -> i32 { unsafe { \
             let r = panic::catch_unwind(|| (*h).work()); if r.is_ok() { 0 } else { -1 } } }";
        assert!(run(source).is_empty());
    }

    /// A body reduced to a field read cannot panic on its own.
    #[test]
    fn allows_trivial_field_read_body() {
        let source = "pub extern \"C\" fn version(h: &Handle) -> u32 { h.version }";
        assert!(run(source).is_empty());
    }

    /// A cast and a comparison cannot panic either.
    #[test]
    fn allows_trivial_cast_and_comparison_body() {
        let source = "pub extern \"C\" fn is_set(flags: u32) -> i32 { (flags != 0) as i32 }";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_constant_body() {
        let source = "pub extern \"C\" fn abi_version() -> i32 { 3 }";
        assert!(run(source).is_empty());
    }

    /// A declaration in an `extern "C" { … }` block has no body to guard.
    #[test]
    fn allows_extern_block_declaration() {
        let source = "extern \"C\" { pub fn strlen(s: *const i8) -> usize; }";
        assert!(run(source).is_empty());
    }

    /// An ordinary Rust function is not an FFI boundary, whatever it does.
    #[test]
    fn allows_plain_rust_fn_that_unwraps() {
        let source = "pub fn parse(p: &str) -> usize { p.parse::<usize>().unwrap() }";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_extern_c_fn_in_test_module() {
        let source = "#[cfg(test)]\nmod tests { \
             pub extern \"C\" fn cb(n: i32) -> i32 { n.checked_add(1).unwrap() } }";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_extern_c_fn_in_tests_directory() {
        let source = "pub extern \"C\" fn cb(v: &[u8]) -> u8 { v[0] }";
        assert!(crate::rules::test_helpers::run_rule(&Check, source, "tests/ffi.rs").is_empty());
    }
}
