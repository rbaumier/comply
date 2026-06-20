//! rust-no-panic-macros backend.
//!
//! Flags invocations of `panic!`, `todo!`, `unimplemented!`, and
//! `unreachable!` outside of test code. These macros all abort at
//! runtime — the opposite of what a production service should do.
//!
//! - `panic!` — turn it into a typed `Result` error. Exception: a `panic!`
//!   that is the *entire* body of a trait-impl method whose return type forces
//!   a value (not `Result`/`Option`/`()`) is the null-object pattern — the
//!   trait signature makes any non-panicking implementation impossible, so the
//!   panic is the only correct response to a documented invariant violation.
//! - `todo!` / `unimplemented!` — placeholders that must not ship.
//! - `unreachable!` — asserts an invariant the compiler can't prove. A
//!   documented `unreachable!("reason")` carrying an explanatory string
//!   message is allowed; a bare, undocumented `unreachable!()` is not.
//!
//! Tests are exempted because panicking in a `#[test]` is a clean
//! failure mode. Same exemption logic as `rust-no-unwrap`. cargo-fuzz
//! targets (files under a `fuzz_targets/` directory) are also exempt:
//! in a libfuzzer-sys target, `panic!` is the deliberate
//! crash-signaling mechanism the fuzzer catches to report a found bug.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{
    enclosing_fn, is_in_test_context, is_in_trait_impl, is_under_tests_dir, macro_body,
    split_top_level_args, string_literal_content,
};

const KINDS: &[&str] = &["macro_invocation"];

const BANNED_MACROS: &[&str] = &["panic", "todo", "unimplemented", "unreachable"];

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
        let source_bytes = ctx.source.as_bytes();
        let Some(macro_name_node) = node.child_by_field_name("macro") else {
            return;
        };
        let Ok(macro_name) = macro_name_node.utf8_text(source_bytes) else {
            return;
        };
        if !BANNED_MACROS.contains(&macro_name) {
            return;
        }
        // Dual-read: the unit-test harness injects an empty default FileCtx, so
        // `in_fuzz_targets` is false in tests — fall back to the pure path
        // predicate, which reads `ctx.path` directly.
        if is_in_test_context(node, source_bytes)
            || is_under_tests_dir(ctx.path)
            || ctx.file.path_segments.in_fuzz_targets
            || crate::rules::path_utils::is_fuzz_targets_path(ctx.path)
        {
            return;
        }
        // `unreachable!` asserts an invariant, not a reachable failure. A
        // documented `unreachable!("reason")` carrying an explanatory string
        // message is the legitimate form — exempt it. A bare `unreachable!()`
        // (or one whose first argument is not a string literal) still flags.
        // The other three macros are unaffected: a message does not make "this
        // can happen / isn't done" acceptable.
        if macro_name == "unreachable" && has_documented_message(node, ctx.source) {
            return;
        }
        // Null-object pattern: a `panic!` that is the *entire* body of a
        // trait-impl method returning a bare value type. The implementor
        // can't change the signature (it's the trait contract), the return
        // type isn't `Result`/`Option`/`()` so `?`/`Err`/`None`/early-return
        // are impossible, and the method does nothing but panic — calling it
        // is a documented invariant violation (e.g. `get_val` on an empty
        // column sentinel). Same justification as documented `unreachable!`:
        // the arm has no value to return. A `panic!` buried in real logic, or
        // in a method whose signature admits a non-panicking result, still
        // flags. Restricted to `panic!`: `todo!`/`unimplemented!` are
        // placeholders that must not ship even as a sole-body stub.
        if macro_name == "panic" && is_sole_body_null_object_panic(node, source_bytes) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-panic-macros".into(),
            message: format!(
                "`{macro_name}!` aborts at runtime. Replace with a typed \
                 `Result` error. `todo!`/`unimplemented!` are placeholders \
                 that must not ship; a bare `unreachable!()` needs a \
                 documenting message — write `unreachable!(\"reason\")` to \
                 assert the invariant. Tests are exempted."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True when an `unreachable!` invocation documents its invariant with an
/// explanatory string-literal message, i.e. its first argument is a string
/// literal (`unreachable!("reason")` / `unreachable!("BUG: {:?}", err)`). A
/// bare `unreachable!()` or one whose first argument is not a string literal
/// returns false.
fn has_documented_message(node: tree_sitter::Node, source: &str) -> bool {
    let Ok(text) = node.utf8_text(source.as_bytes()) else {
        return false;
    };
    let Some(body) = macro_body(text) else {
        return false;
    };
    let Some(first_arg) = split_top_level_args(body).into_iter().next() else {
        return false;
    };
    string_literal_content(first_arg.trim()).is_some()
}

/// True when `panic_node` is the sole body of a trait-impl method whose return
/// type forces a value — the structural "null object" shape where `panic!` is
/// the only correct implementation.
///
/// All three must hold:
/// 1. The panic sits inside an `impl Trait for Type` method ([`is_in_trait_impl`]):
///    the implementor can't widen the signature to `Result`/`Option`.
/// 2. The method's `return_type` is an infallible value type — not `Result<…>`,
///    not `Option<…>`, not unit `()`. Those three admit a non-panicking answer
///    (`Err`/`None`/do-nothing), so a panic there is a real choice, not forced.
/// 3. The panic *is* the method body — the block's single statement/tail
///    expression is the `panic!` itself (modulo an `expression_statement`
///    wrapper). A `panic!` in one arm of a compound sole expression
///    (`if`/`match`/`let-else`) is not exempt: the other arm is a real
///    non-panicking path, so the panic is a choice, not forced. Likewise a
///    `panic!` following other statements flags.
fn is_sole_body_null_object_panic(panic_node: tree_sitter::Node, source: &[u8]) -> bool {
    if !is_in_trait_impl(panic_node) {
        return false;
    }
    let Some(func) = enclosing_fn(panic_node) else {
        return false;
    };
    if !returns_infallible_value(func, source) {
        return false;
    }
    body_sole_expression(func).is_some_and(|expr| unwrap_expr_stmt(expr).id() == panic_node.id())
}

/// Unwraps a single `expression_statement` wrapper to its inner expression. A
/// tail `panic!()` with no trailing `;` is the `macro_invocation` directly; one
/// with a `;` is wrapped in an `expression_statement`. Returns `node` unchanged
/// when it is not such a wrapper.
fn unwrap_expr_stmt(node: tree_sitter::Node) -> tree_sitter::Node {
    if node.kind() == "expression_statement" {
        node.named_child(0).unwrap_or(node)
    } else {
        node
    }
}

/// True if `func`'s `return_type` is a bare value type — anything other than
/// `Result<…>`, `Option<…>`, or unit `()`. A method with no `return_type` (an
/// implicit `()` return) returns false: it could simply do nothing. The match
/// is on the type's last path segment, so qualified forms like
/// `std::result::Result` and `anyhow::Result` are recognized too.
fn returns_infallible_value(func: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(ret) = func.child_by_field_name("return_type") else {
        return false;
    };
    let Ok(text) = ret.utf8_text(source) else {
        return false;
    };
    let text = text.trim();
    if text == "()" {
        return false;
    }
    let head = text.split(['<', ' ']).next().unwrap_or(text);
    let last_segment = head.rsplit("::").next().unwrap_or(head);
    !matches!(last_segment, "Result" | "Option")
}

/// The single meaningful expression of `func`'s body block, or `None` when the
/// block is empty or holds more than one statement/expression. Used to confirm
/// the method does nothing but its one expression.
fn body_sole_expression(func: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let body = func.child_by_field_name("body")?;
    let mut cursor = body.walk();
    let mut sole = None;
    for child in body.named_children(&mut cursor) {
        if child.kind() == "line_comment" || child.kind() == "block_comment" {
            continue;
        }
        if sole.is_some() {
            return None;
        }
        sole = Some(child);
    }
    sole
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
    fn flags_panic_macro() {
        assert_eq!(run_on(r#"fn f() { panic!("boom"); }"#).len(), 1);
    }

    #[test]
    fn flags_todo_macro() {
        assert_eq!(run_on("fn f() { todo!(); }").len(), 1);
    }

    #[test]
    fn flags_unimplemented_macro() {
        assert_eq!(run_on("fn f() { unimplemented!(); }").len(), 1);
    }

    #[test]
    fn flags_unreachable_macro() {
        assert_eq!(run_on("fn f() { unreachable!(); }").len(), 1);
    }

    #[test]
    fn allows_documented_unreachable_with_message() {
        // gitoxide gix-config/src/key.rs:117 — an invariant the compiler can't
        // prove but the message documents; the arm has no value to return.
        let source = r#"fn f() { unreachable!("iterator can't restart producing items"); }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_documented_unreachable_with_format_message() {
        // gitoxide gix-config/src/file/includes/mod.rs:135.
        let source =
            r#"fn f() { unreachable!("BUG: {:?} not possible due to no-follow options", err); }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_bare_unreachable_without_message() {
        // No documented invariant — an undocumented `unreachable!()` still flags.
        assert_eq!(run_on("fn f() { unreachable!(); }").len(), 1);
    }

    #[test]
    fn flags_unreachable_with_non_string_first_arg() {
        // A non-string first argument is not a documented-invariant message.
        assert_eq!(run_on("fn f() { unreachable!(code); }").len(), 1);
    }

    #[test]
    fn allows_documented_unreachable_with_padded_message() {
        // Whitespace between the delimiter and the message must not defeat the
        // exemption — the argument is trimmed before the literal check.
        assert!(run_on(r#"fn f() { unreachable!( "padded reason" ); }"#).is_empty());
    }

    #[test]
    fn allows_null_object_panic_in_infallible_trait_method() {
        // tantivy columnar/src/column_values/mod.rs:192 — the FP from #4781.
        // `get_val` returns `T` (infallible), the impl is for the empty-column
        // sentinel, and the panic is the method's entire body.
        let source = r#"
            impl<T: PartialOrd + Default> ColumnValues<T> for EmptyColumnValues {
                fn get_val(&self, _idx: u32) -> T {
                    panic!("Internal Error: Called get_val of empty column.")
                }
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_panic_in_trait_method_returning_result() {
        // The signature admits `Err` — the panic is a real choice, not forced.
        let source = r#"
            impl Reader for EmptyReader {
                fn read(&self) -> Result<T, E> {
                    panic!("nothing to read")
                }
            }
        "#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_panic_in_trait_method_returning_option() {
        // `Option` admits `None` — a non-panicking answer exists.
        let source = r#"
            impl Lookup for EmptyMap {
                fn find(&self, _k: u32) -> Option<T> {
                    panic!("nothing to find")
                }
            }
        "#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_conditional_panic_in_infallible_trait_method() {
        // The panic is buried in real logic, not the sole body — normal code.
        let source = r#"
            impl ColumnValues<T> for SparseColumn {
                fn get_val(&self, idx: u32) -> T {
                    if idx >= self.len {
                        panic!("out of bounds");
                    }
                    self.data[idx as usize]
                }
            }
        "#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_sole_body_panic_in_inherent_impl() {
        // No trait contract forces the signature — the author could return a
        // `Result` instead, so the panic still flags.
        let source = r#"
            impl EmptyColumn {
                fn get_val(&self, _idx: u32) -> T {
                    panic!("Called get_val of empty column.")
                }
            }
        "#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_panic_in_compound_sole_expression_branch() {
        // The body is one `if/else` — the `else` returns a real value, so the
        // panic is a choice, not forced. Not the null-object shape.
        let source = r#"
            impl ColumnValues<T> for MaybeColumn {
                fn get_val(&self, idx: u32) -> T {
                    if let Some(v) = self.data.get(idx) { v } else { panic!("missing") }
                }
            }
        "#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_panic_in_match_sole_expression_arm() {
        // A `match` whose other arm yields a value — the panic is not forced.
        let source = r#"
            impl Reader for StateReader {
                fn read(&self) -> T {
                    match self.state {
                        State::Ready(v) => v,
                        _ => panic!("not ready"),
                    }
                }
            }
        "#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_sole_body_panic_in_trait_method_returning_qualified_result() {
        // `std::result::Result` admits `Err` just like bare `Result` — the
        // last path segment is matched, so the panic still flags.
        let source = r#"
            impl Reader for EmptyReader {
                fn read(&self) -> std::result::Result<T, E> {
                    panic!("nothing to read")
                }
            }
        "#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_sole_body_panic_in_unit_returning_trait_method() {
        // A `()` return could simply do nothing — the panic is a choice.
        let source = r#"
            impl Sink for NullSink {
                fn write(&self, _data: &[u8]) {
                    panic!("null sink cannot write")
                }
            }
        "#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_documented_panic_with_message() {
        // A message does not exempt `panic!` — it can still happen at runtime.
        assert_eq!(run_on(r#"fn f() { panic!("boom"); }"#).len(), 1);
    }

    #[test]
    fn flags_documented_unimplemented_with_message() {
        // A message does not exempt `unimplemented!` — it must not ship.
        assert_eq!(run_on(r#"fn f() { unimplemented!("later"); }"#).len(), 1);
    }

    #[test]
    fn allows_panic_in_test_fn() {
        let source = "#[test]\nfn it_panics() { panic!(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_in_cfg_test_module() {
        let source = "#[cfg(test)]\nmod tests { fn helper() { panic!(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_println() {
        assert!(run_on(r#"fn f() { println!("hi"); }"#).is_empty());
    }

    #[test]
    fn allows_panic_in_tokio_test() {
        let source = "#[tokio::test]\nasync fn it_works() { panic!(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_in_actix_rt_test() {
        let source = "#[actix_rt::test]\nasync fn it_works() { panic!(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_in_tests_directory() {
        let source = "fn helper() { panic!(); }";
        assert!(crate::rules::test_helpers::run_rule(&Check, source, "tests/helpers.rs").is_empty());
    }

    #[test]
    fn allows_panic_in_fuzz_target() {
        let source = r#"fn run() { panic!("should be able to parse a printed value"); }"#;
        assert!(crate::rules::test_helpers::run_rule(
            &Check,
            source,
            "fuzz/fuzz_targets/rfc2822_parse.rs"
        )
        .is_empty());
    }

    #[test]
    fn flags_panic_in_regular_src() {
        let source = r#"fn f() { panic!("boom"); }"#;
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, source, "src/lib.rs").len(),
            1
        );
    }

    #[test]
    fn allows_panic_in_testing_rs() {
        let source = r#"pub fn h() { panic!("boom"); }"#;
        assert!(
            crate::rules::test_helpers::run_rule(&Check, source, "crates/foo/src/testing.rs")
                .is_empty()
        );
    }

    #[test]
    fn allows_panic_in_test_utils_rs() {
        let source = r#"pub fn h() { panic!("boom"); }"#;
        assert!(
            crate::rules::test_helpers::run_rule(&Check, source, "crates/foo/src/test_utils.rs")
                .is_empty()
        );
    }

    #[test]
    fn allows_panic_in_testutil_rs() {
        // ripgrep's crates/searcher/src/testutil.rs — the FP from #3282.
        let source = r#"pub fn h() { panic!("boom"); }"#;
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
    fn allows_panic_under_testutil_dir() {
        let source = r#"pub fn h() { panic!("boom"); }"#;
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
    fn allows_panic_under_property_tests_dir() {
        let source = r#"pub fn gen() { panic!("boom"); }"#;
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                source,
                "crates/foo/src/types/property_tests/setup.rs"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_panic_in_non_exact_testing_name() {
        let source = r#"pub fn m() { panic!("boom"); }"#;
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, source, "crates/foo/src/my_testing.rs")
                .len(),
            1
        );
    }

    #[test]
    fn flags_panic_in_non_exact_testing_dir() {
        let source = r#"pub fn tg() { panic!("boom"); }"#;
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
}
