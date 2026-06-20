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
//! build.rs is exempted — panics in Cargo build scripts are an acceptable
//! error mode during compilation (e.g. env::var("FOO").unwrap()).
//!
//! Example code is exempted — files under a Cargo `examples/` directory (or a
//! disabled variant like `examples_disabled/`) are illustrative, so `.unwrap()`
//! keeps them concise instead of obscuring the demonstrated feature with error
//! plumbing.
//!
//! Lock operations are exempted — `.read().unwrap()`, `.write().unwrap()`,
//! `.lock().unwrap()` are idiomatic for std::sync::{Mutex,RwLock} poisoning.
//!
//! Fixed-size-key delegation is exempted — `Self::new_from_slice(key).unwrap()`
//! where `key` is a parameter typed `&Key<…>` (a RustCrypto `GenericArray`
//! whose length is fixed by the type) cannot fail the length check, so the
//! unwrap is infallible. This is the prescribed `KeyInit::new` implementation
//! shape across `RustCrypto/block-ciphers`. The arg must be a `&Key<…>`-typed
//! parameter; `new_from_slice` on an arbitrary `&[u8]` still flags.
//!
//! This rule is equivalent to `clippy::unwrap_used` + `clippy::expect_used`
//! (both restriction-group lints, off by default in clippy). Running it
//! via comply means you get the check without having to enable the lints
//! in every consuming crate.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::path_utils::is_cargo_example_path;
use crate::rules::rust_helpers::{is_in_const_initializer, is_in_test_context, is_under_tests_dir};

const KINDS: &[&str] = &["call_expression"];

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
        // Skip example code — `.unwrap()` keeps examples concise.
        if is_cargo_example_path(ctx.path) {
            return;
        }
        // Skip const/static item initializers — `unwrap`/`expect` is const-evaluated
        // at compile time and is the only valid way to extract the value there.
        if is_in_const_initializer(node) {
            return;
        }
        // Skip lock operations — .read()/.write()/.lock()/.try_lock() unwrap is idiomatic.
        if field_text == "unwrap" {
            let receiver = function.child_by_field_name("value");
            if let Some(recv) = receiver {
                if recv.kind() == "call_expression" {
                    if let Some(inner_func) = recv.child_by_field_name("function") {
                        if inner_func.kind() == "field_expression" {
                            if let Some(inner_field) = inner_func.child_by_field_name("field") {
                                if let Ok(method) = inner_field.utf8_text(source_bytes) {
                                    if matches!(method, "read" | "write" | "lock" | "try_lock" | "try_read" | "try_write") {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Skip RustCrypto `KeyInit::new` delegation — `new_from_slice(key).unwrap()`
        // where `key` is a `&Key<…>`-typed parameter cannot fail the length check.
        if field_text == "unwrap" && is_fixed_size_key_delegation(function, node, source_bytes) {
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

    #[test]
    fn flags_unwrap_in_production_fn() {
        assert_eq!(run_on("fn f() { let x = y.unwrap(); }").len(), 1);
    }

    #[test]
    fn flags_expect_in_production_fn() {
        assert_eq!(run_on(r#"fn f() { let x = y.expect("msg"); }"#).len(), 1);
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
    fn flags_unwrap_in_const_fn_body() {
        // A `const fn` body is a runtime body that can return `Result` / use `?`,
        // so unwrap there is still flagged.
        let source = "const fn f(x: Option<u32>) -> u32 { x.unwrap() }";
        assert_eq!(run_on(source).len(), 1);
    }
}
