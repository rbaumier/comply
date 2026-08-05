//! no-ignored-exceptions Rust backend — flag `let _ = fallible()` that
//! discards a Result/Option without handling it.
//!
//! Tests, Kani harnesses, and benchmarks are exempted: a `let _ =
//! fn_under_test()` pattern is the idiomatic way to assert "this call doesn't
//! panic" without caring about the return value. Exempt when in a test context
//! — a `#[test]`/`#[cfg(test)]` attribute walk via
//! `rust_helpers::is_in_test_context` — when inside a Kani verification harness
//! (an enclosing fn carrying `#[kani::proof]`/`#[kani::proof_for_contract(...)]`,
//! via `rust_helpers::is_in_kani_proof`), where `let _ = f(kani::any())` is the
//! conventional way to exercise a call for its safety property — or when the file
//! lives under a `tests/` directory (`rust_helpers::is_under_tests_dir`), since
//! plain helper fns there are integration-test infrastructure. Benchmark files
//! (`FileCtx::in_benchmark_dir`) are exempt too: in a Criterion `b.iter(|| { let
//! _ = f(black_box(..)) })` closure the discard is the idiomatic way to run a
//! call for its timing without consuming the result, and benchmark code never
//! ships to production. Fuzz-harness files (via `ProjectCtx::in_fuzz_crate`:
//! a `fuzz_targets/` directory segment, or a fuzz crate identified by its
//! `Cargo.toml` — `[package.metadata] cargo-fuzz = true` or a `libfuzzer-sys`/
//! `afl`/`honggfuzz` dependency) are exempt for the same reason: `let _ =
//! f(fuzzer_input)` is the canonical idiom for exercising a function under
//! fuzzing — success vs. failure is deliberately ignored, only "does not panic"
//! matters, and fuzz harnesses never ship to production. Keying on the manifest
//! covers non-`fuzz_targets/` layouts (cargo-fuzz `fuzz/fuzzers/`, AFL
//! `fuzz-afl/{fuzzers,reproducers}/`).
//!
//! Four non-error idioms are also exempted:
//! - `let _ = expr?`: the `?` operator already propagates any `Err`/`None` to
//!   the caller, so the error is handled — only the unwrapped success value is
//!   discarded (e.g. `let _ = parser.expect(kw)?` checks a token exists then
//!   drops it).
//! - `let _ = Arc::from_raw(p)` / `Box::from_raw(p)` (and bare `from_raw`):
//!   reconstructing an owning pointer from a raw pointer and dropping it to
//!   run its `Drop` impl. The reconstruction is infallible — `let _ =` invokes
//!   the destructor, it does not ignore an error.
//! - compile-fail test fixtures under a `tests/.../fail/` directory: `let _ =`
//!   suppresses "unused result" warnings so they don't pollute the expected
//!   compiler error output of `trybuild`/`tests-build` cases.
//! - `let _ = expr.send(..)`: the best-effort channel fire-and-forget idiom on
//!   a `oneshot`/`mpsc` sender. An `Err` from `send` only signals the receiver
//!   already dropped (shutdown/cleanup path), which is intentionally ignored.
//! - `let _ = stderr()/stdout().write_all(..)` (also `write`/`write_fmt`/`flush`):
//!   a best-effort write to a standard stream. These run in error-reporting
//!   paths (panic/signal handlers, `Drop`, fallback loggers) where an I/O error
//!   has nowhere to be propagated or reported, so `let _ =` intentionally drops
//!   it. Scoped to receiver chains rooted at `stderr()`/`stdout()`, seen through
//!   any intermediate call (`.lock()`) and through the bindings that name the
//!   handle (`let out = io::stdout(); let mut lock = out.lock();`), so a plain
//!   `let _ = file.write_all(..)` still fires. The macro spellings of the same
//!   idiom are also exempt: `print!`/`println!`/`eprint!`/`eprintln!` target a
//!   standard stream by definition, and `write!`/`writeln!` when their first
//!   argument (the writer) roots at `stderr()`/`stdout()` the same way — so a
//!   plain `let _ = writeln!(file, ..)` to an ordinary writer still fires.
//! - `let _ = expr.write_str(..)` / `let _ = expr.write_char(..)`: the
//!   `core::fmt::Write` trait methods. Writing to an in-memory buffer (`String`,
//!   `fmt::Formatter`, a `Vec<u8>` wrapper) yields a `fmt::Result` that is
//!   structurally `Ok(())`, so discarding it with `let _ =` drops an always-Ok
//!   unit, not an error. The method names `write_str`/`write_char` are specific
//!   to `fmt::Write` (distinct from `io::Write`'s `write`/`write_all`), so this
//!   is exempt by method name without type inference — a plain
//!   `let _ = file.write_all(..)` still fires.
//! - `let _ = write!(buf, ..)` / `let _ = writeln!(buf, ..)` where `buf` resolves
//!   to an in-memory buffer — the macro sibling of the `write_str`/`write_char`
//!   exemption. An in-memory write yields a `Result` that is structurally
//!   `Ok(())`, so `let _ =` drops an always-Ok unit, not an error. The writer is
//!   the first macro argument; it is exempt only when it provably resolves (from
//!   the AST) to a `String`/`fmt::Formatter`/`Vec<u8>`: a parameter/`let`
//!   annotated `String`/`&mut String`/`fmt::Formatter`/`Vec<u8>`, or a binding
//!   whose value comes from a `String::…`/`format!(…)`/`<expr>.to_string()`/
//!   `Vec::…`/`vec![…]` expression. A fallible `io::Write` writer (`File`,
//!   `BufWriter`, socket) or an unresolvable writer still fires.
//! - `let _ = expr.ok()` / `let _ = expr.err()`: the terminal method is the
//!   `Result::ok`/`Result::err` combinator, which converts a `Result<T, E>` into
//!   an `Option<T>`/`Option<E>`, structurally discarding the opposite arm. Its
//!   return type is `Option`, never `Result`, so once `.ok()`/`.err()` has
//!   stripped the error the value bound by `let _ =` carries none to handle.
//!   Exempt by the terminal method name (like `write_str`/`write_char`) without
//!   type inference — a bare `let _ = fallible()` or a chain whose terminal call
//!   returns a `Result` (`let _ = expr.lookup()`) still fires.
//! - `let _ = expr.map_err(|e| ...)` / `expr.inspect_err(|e| ...)`: the error is
//!   explicitly observed/handled (e.g. logged) inside the closure before the
//!   resulting `Result` is intentionally discarded. The closure argument is
//!   required, so a `map_err(some_fn)` taking a bare function (which may swallow
//!   the error) still fires. The method may sit anywhere in the call chain.
//! - `let _ = expr.<method>(..)` where `<method>` is a curated std-collection
//!   method that returns `Option`/`bool`/`()` and never `Result`
//!   (`remove`, `insert`, `pop`, `push`, `take`, …; see `NON_RESULT_METHODS`).
//!   These carry no error, so discarding the return value ignores nothing
//!   (e.g. `let _ = values.remove("inherits")` drops the previous `Option<V>`).
//!   Scoped to the method-call shape on a receiver, so the free function
//!   `let _ = std::fs::remove_file(p)` (returns `io::Result`) still fires.
//! - `let _ = fallible()` inside a destructor body — `fn drop` of an
//!   `impl Drop for ...` block, or `async fn async_drop` of an
//!   `impl AsyncDrop for ...` block (the `async_dropper` / nightly
//!   `std::future::AsyncDrop` async-destructor convention): both return `()`, so
//!   an error has no way to be propagated to the caller (no `?`, no `Result`
//!   return). `let _ =` is the idiomatic best-effort cleanup for an RAII
//!   destructor. Scoped to the destructor method paired with its trait, so a
//!   `let _ =` in any other method still fires.
//! - `let _ = fallible()` in an exit-imminent scope: the discard's enclosing
//!   block also contains a `process::exit(..)` call (e.g. best-effort terminal
//!   cleanup in a `ctrlc::set_handler` closure that exits with code 130). After
//!   `process::exit` the process terminates, so the error cannot be propagated
//!   or handled — structurally identical to the `Drop::drop` case. Anchored on a
//!   `process::exit` / `std::process::exit` call (a `scoped_identifier` callee
//!   whose tail is `exit` qualified by `process`) anywhere in the same block,
//!   including a nested `if`/`match` arm — so a conditional exit also exempts. An
//!   ordinary discard in a block with no exit still fires, a user method
//!   `foo.exit()` does not qualify, and an exit buried in a nested closure does
//!   not exempt a sibling discard in the outer block.
//! - `let _ = fallible()` inside a function whose return type is `!` (the never
//!   type): a `-> !` function is compiler-guaranteed to diverge — it never
//!   returns to a caller (it ends in `process::exit`, `panic!`, an infinite
//!   loop, …), so a dropped error has no caller to propagate to. Detected via
//!   `rust_helpers::is_in_never_fn`, which walks to the nearest enclosing
//!   `function_item` and checks its `return_type` is a `never_type` node. This
//!   divergence guarantee is structurally stronger than the block-level
//!   exit-imminent scan above: it also covers fd's `ExitCode::exit` shape where
//!   `process::exit` sits at the fn level while the `let _ =` is nested inside
//!   `unsafe {}`/`if`. A `-> Result<!, E>` (where `!` is buried in a generic) is
//!   a `generic_type`, not a bare `never_type`, so it does not qualify, and a fn
//!   returning `()`/`Result<…>`/any value type still fires.
//! - `let _ = f::<Infallible, _>(..)`: a turbofish call whose type arguments fix
//!   the error type to `Infallible`. `Result<_, Infallible>` is uninhabited on
//!   its `Err` side, so the result can never be `Err` — discarding it ignores no
//!   error. Recognized purely syntactically on the turbofish: any type argument
//!   whose final path segment is `Infallible` (`Infallible`,
//!   `std::convert::Infallible`, `core::convert::Infallible`). A turbofish with a
//!   real error type (`let _ = f::<MyError, _>(..)`) or no turbofish at all still
//!   fires.
//! - `let _ = macro!(var in stream)`: an output-parameter binding macro
//!   (`syn::parenthesized!(content in input)`, `braced!`, `bracketed!`). These
//!   parsing macros write their primary parse result into the pre-declared
//!   `var` binding (what the caller consumes) and return only secondary
//!   delimiter-span metadata, so `let _ =` discards the metadata, not an error.
//!   Keyed purely on the token sequence `identifier`/`in`/`identifier` among the
//!   macro token tree's direct children — no macro-name allowlist — so a plain
//!   `let _ = macro!(a, b)` with no `in` binding still fires.
//!
//! A named writer is resolved once, for the buffer exemption and the std-stream
//! exemption alike, and in the macro spelling and the method spelling alike:
//! `rust_helpers::local_binding_init_expression` answers which expression the
//! binding takes its value from — the initializer of the `let` that binds it, or
//! the seed a `fold`/`try_fold`/`scan` passes alongside the closure whose
//! parameter it is (that function's doc holds the reason those three signatures
//! resolve and other calls of the same shape do not). So
//! `.fold(String::new(), |mut out, x| { let _ = writeln!(out, ..); out })` reads
//! as the `String` it is, and `let mut err = io::stderr()` reads as the handle an
//! inline `io::stderr()` already did. Each exemption then classifies that one
//! expression, so a binding form either can reach is one both reach.
//!
//! The four write/send idioms above are matched where the `Err` is born, not
//! only on the outermost call, so a producer wrapped in a combinator
//! (`let _ = iter.try_for_each(|x| tx.send(x))`) is exempt too. See
//! [`has_unhandleable_error_origin`] for the links followed and the false
//! negatives that search accepts.
//!
//! NOTE: This rule uses a heuristic (call-like pattern matching) rather than
//! type awareness. It may flag `let _ = infallible_fn()` where the function
//! provably does not return Result/Option. Without --type-aware, there is no
//! fix for this class of FP — document intent in the calling code if needed.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{
    expression_yields_buffer, find_identifier_type, is_in_kani_proof, is_in_never_fn,
    is_in_test_context, is_under_tests_dir, local_binding_init_expression, trait_base_name,
};
use tree_sitter::Node;

crate::ast_check! { on ["let_declaration"] => |node, source, ctx, diagnostics|
    // Check if the pattern is `_` (wildcard).
    let Some(pattern) = node.child_by_field_name("pattern") else { return };
    let Ok(pat_text) = pattern.utf8_text(source) else { return };
    if pat_text != "_" {
        return;
    }

    // Must have a value (right-hand side).
    let Some(value) = node.child_by_field_name("value") else { return };

    // `let _ = expr?`: the `?` already propagates the error to the caller, so
    // only the unwrapped success value is discarded — not an ignored error.
    if value.kind() == "try_expression" {
        return;
    }

    // The value should be a call expression or method call (likely fallible).
    let is_call = matches!(
        value.kind(),
        "call_expression" | "macro_invocation" | "await_expression"
            | "field_expression"
    );
    if !is_call {
        return;
    }

    // Skip inside tests, Kani harnesses, benchmarks, and fuzz harnesses —
    // `let _ = …` there is "call and don't care". Covers a `#[test]`/`#[cfg(test)]`
    // attribute context, a Kani verification harness
    // (`#[kani::proof]`/`#[kani::proof_for_contract]`), a file under a `tests/`
    // directory (plain helper fns there are test infrastructure), benchmark files
    // (`benches/`, `_bench.rs`) where a Criterion `b.iter` closure discards the
    // result to time the call, and fuzz-harness files (a `fuzz_targets/`
    // directory segment or a fuzz crate identified by its `Cargo.toml`, via
    // `ProjectCtx::in_fuzz_crate`) where `let _ = f(fuzzer_input)` exercises a
    // call only to assert it does not panic.
    if is_in_test_context(node, source)
        || is_in_kani_proof(node, source)
        || is_under_tests_dir(ctx.path)
        || ctx.file.in_benchmark_dir()
        || ctx.project.in_fuzz_crate(ctx.path)
    {
        return;
    }

    // Skip compile-fail test fixtures (`tests/.../fail/`): `let _ =` there
    // suppresses "unused result" warnings in the expected compiler output.
    if is_compile_fail_fixture(ctx.path) {
        return;
    }

    // Skip an output-parameter binding macro `let _ = macro!(var in stream)`
    // (e.g. `syn::parenthesized!(content in input)`): its primary parse result
    // is written into the pre-declared `var`, and the discarded return value is
    // only secondary delimiter metadata — not an error.
    if macro_uses_output_param_binding(value, source) {
        return;
    }

    // Skip the intentional-drop idiom `let _ = Arc/Box::from_raw(p)`: the
    // reconstruction is infallible and exists only to run the value's `Drop`.
    if is_from_raw_reconstruction(value, source) {
        return;
    }

    // Skip a turbofish call fixing the error type to `Infallible`
    // (`let _ = f::<Infallible, _>(..)`): the `Err` side is uninhabited, so the
    // result can never be `Err` — there is no error to handle.
    if has_infallible_turbofish(value, source) {
        return;
    }

    // Skip a discard whose `Err` is produced by an operation that cannot act on
    // it — a channel send, a standard-stream write, a `core::fmt::Write` call,
    // an in-memory `write!` — wherever in the expression that operation sits.
    if has_unhandleable_error_origin(value, source) {
        return;
    }

    // Skip the Result→Option error-discard combinator `let _ = expr.ok()` /
    // `expr.err()`: `Result::ok`/`Result::err` convert the `Result` into an
    // `Option`, structurally dropping the opposite arm, so the discarded value
    // carries no error to handle.
    if is_result_to_option_combinator(value, source) {
        return;
    }

    // Skip the handled-then-discarded idiom `let _ = expr.map_err(|e| ...)`:
    // the error is observed inside the `map_err`/`inspect_err` closure before
    // the now-trivial `Result` is intentionally dropped.
    if chain_has_error_handling_closure(value, source) {
        return;
    }

    // Skip discards of std-collection methods that return `Option`/`bool`/`()`
    // (`let _ = map.remove(k)`): these carry no error, so nothing is ignored.
    if is_non_result_std_method(value, source) {
        return;
    }

    // Skip best-effort cleanup in a destructor body (`Drop::drop` /
    // `AsyncDrop::async_drop`): both return `()`, so the error has no way to be
    // propagated — `let _ =` is the idiomatic form.
    if is_in_drop_impl(node, source) {
        return;
    }

    // Skip best-effort cleanup in an exit-imminent scope: the enclosing block
    // also calls `process::exit(..)`, after which the process terminates and no
    // error can be handled — structurally identical to `Drop::drop`.
    if is_in_process_exit_scope(node, source) {
        return;
    }

    // Skip discards inside a function whose return type is `!` (never): the
    // compiler guarantees it diverges and never returns to a caller, so a
    // dropped error can provably never propagate — the divergence guarantee is
    // stronger than the block-level `process::exit` scan above and covers a
    // `process::exit` at the fn level while the discard is nested in `unsafe`/
    // `if`.
    if is_in_never_fn(node) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-ignored-exceptions".into(),
        message: "`let _ = ...` discards a potentially fallible result \u{2014} handle the error or use `drop()`.".into(),
        severity: Severity::Error,
        span: None,
    });
}

/// True for compile-fail fixtures: a `fail` directory component nested under a
/// `tests` component (`tests/fail/`, `tests-build/tests/fail/`,
/// `*_compile_tests/tests/fail/`). Both components must be present so ordinary
/// `fail/` directories outside a test harness are still checked.
fn is_compile_fail_fixture(path: &std::path::Path) -> bool {
    let mut seen_tests = false;
    for component in path.components() {
        let segment = component.as_os_str();
        if segment == "tests" {
            seen_tests = true;
        } else if segment == "fail" && seen_tests {
            return true;
        }
    }
    false
}

/// True if `node` sits inside a destructor body: `fn drop` of an
/// `impl Drop for ...` block, or `async fn async_drop` of an
/// `impl AsyncDrop for ...` block.
///
/// Walks ancestors to the nearest enclosing `function_item`, then requires a
/// destructor method paired with its trait: a method named `drop` whose nearest
/// enclosing `impl_item` implements `Drop`, or a method named `async_drop` whose
/// nearest enclosing `impl_item` implements `AsyncDrop` (the async-destructor
/// convention of the `async_dropper` crate and nightly `std::future::AsyncDrop`).
/// The pairing keeps method name and trait matched: `async_drop` under `Drop` (or
/// a non-`AsyncDrop` trait), `drop` under `AsyncDrop`, either name in an inherent
/// impl, or any other method in a destructor impl does not qualify. The trait's
/// final path segment is compared, so `impl std::ops::Drop for T` also matches.
/// In both `Drop::drop` and `AsyncDrop::async_drop` the return type is `()`, so a
/// fallible result discarded with `let _ =` is the idiomatic best-effort cleanup
/// — there is no channel to propagate the error.
fn is_in_drop_impl(node: Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "function_item" {
            let Some(name) = ancestor.child_by_field_name("name") else {
                return false;
            };
            let expected_trait = match name.utf8_text(source) {
                Ok("drop") => "Drop",
                Ok("async_drop") => "AsyncDrop",
                _ => return false,
            };
            return enclosing_impl_is_trait(ancestor, source, expected_trait);
        }
        current = ancestor.parent();
    }
    false
}

/// True if the nearest `impl_item` enclosing `func` is a trait impl whose
/// trait's final path segment equals `expected_trait`.
fn enclosing_impl_is_trait(func: Node, source: &[u8], expected_trait: &str) -> bool {
    let mut current = func.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "impl_item" {
            return ancestor
                .child_by_field_name("trait")
                .and_then(|t| trait_base_name(t, source))
                .is_some_and(|name| name == expected_trait);
        }
        current = ancestor.parent();
    }
    false
}

/// True if `node` sits in an exit-imminent scope: the nearest enclosing block
/// also contains a `process::exit(..)` call somewhere below it.
///
/// Walks up to the nearest `block` ancestor and scans it for a call whose callee
/// is a `scoped_identifier` ending in `exit` qualified by `process`
/// (`process::exit`, `std::process::exit`). The scan descends into nested
/// expressions (an `if`/`match` arm), so a conditional exit also exempts; it
/// stops at nested fn/closure boundaries. After `process::exit` the process
/// terminates, so any error discarded with `let _ =` in the same block cannot be
/// propagated or handled — best-effort cleanup before shutdown, structurally
/// like `Drop::drop`. Anchoring on the `process::exit` call shape (not a method
/// `foo.exit()`, not a bare `exit()`) keeps the exemption tight: an ordinary
/// discard in a block with no such call still fires.
fn is_in_process_exit_scope(node: Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "block" {
            return block_calls_process_exit(ancestor, source);
        }
        current = ancestor.parent();
    }
    false
}

/// True if any descendant call within `block` is `process::exit(..)`. Scans the
/// block's statement expressions (and the nested expressions a `process::exit`
/// commonly hides in, e.g. inside an `if`/`match` arm) for a `call_expression`
/// whose function is a `scoped_identifier` whose final segment is `exit` and
/// whose second-to-last segment is `process`.
fn block_calls_process_exit(block: Node, source: &[u8]) -> bool {
    let mut found = false;
    descend_for_process_exit(block, source, &mut found);
    found
}

/// Recursively walk `node`'s descendants looking for a `process::exit` call,
/// stopping the descent at nested item/closure boundaries that introduce a new
/// scope (`function_item`, `closure_expression`) — a `process::exit` buried in a
/// nested closure does not make the outer block exit-imminent.
fn descend_for_process_exit(node: Node, source: &[u8], found: &mut bool) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if *found {
            return;
        }
        if child.kind() == "call_expression" && call_is_process_exit(child, source) {
            *found = true;
            return;
        }
        // Do not descend into a nested fn or closure: its `process::exit` does
        // not terminate the path of a sibling discard in the outer block.
        if matches!(child.kind(), "function_item" | "closure_expression") {
            continue;
        }
        descend_for_process_exit(child, source, found);
    }
}

/// True if `call` is `process::exit(..)` — a `call_expression` whose function is
/// a `scoped_identifier` whose final segment is `exit` and whose path contains
/// `process` (`process::exit`, `std::process::exit`).
fn call_is_process_exit(call: Node, source: &[u8]) -> bool {
    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "scoped_identifier" {
        return false;
    }
    let Ok(text) = function.utf8_text(source) else {
        return false;
    };
    let mut segments = text.rsplit("::");
    let Some(last) = segments.next() else {
        return false;
    };
    if last.trim() != "exit" {
        return false;
    }
    segments.next().map(str::trim) == Some("process")
}

/// True if `value` is a `macro_invocation` whose token tree uses the
/// output-parameter binding syntax `<output_var> in <input_stream>` — the shape
/// of `syn`'s parsing macros (`parenthesized!(content in input)`, `braced!`,
/// `bracketed!`). These macros write their primary parse result into the
/// pre-declared `output_var` (what the caller consumes) and return only
/// secondary delimiter-span metadata, so `let _ =` discards that metadata, not
/// an error.
///
/// Keyed purely on the token sequence `identifier`/`in`/`identifier` — no
/// macro-name allowlist — so any macro using the same binding shape is exempt
/// while a plain `let _ = macro!(a, b)` (no `in` binding) still fires. syn's
/// parsing macros hold the binding as the SOLE content, so the match is anchored
/// to the first three non-delimiter tokens of the token tree rather than scanned
/// at every position: this keeps an `in` buried in some other macro body (a
/// `for x in y` loop, a trailing `x, y in z`) from matching. The `in` token is
/// matched on its source text rather than a fixed node kind, since inside a
/// token tree tree-sitter may surface it as a keyword token or an identifier.
/// Only the token tree's direct children are read, so a binding nested in a
/// deeper `token_tree` does not match.
fn macro_uses_output_param_binding(value: Node, source: &[u8]) -> bool {
    if value.kind() != "macro_invocation" {
        return false;
    }
    let Some(token_tree) = macro_token_tree(value) else {
        return false;
    };
    // Skip the `(`/`[`/`{` … `)`/`]`/`}` delimiters; the binding's output
    // identifier is the first remaining token.
    let mut token_cursor = token_tree.walk();
    let mut tokens = token_tree
        .children(&mut token_cursor)
        .filter(|child| !matches!(child.kind(), "(" | ")" | "[" | "]" | "{" | "}"));
    let (Some(output_var), Some(keyword), Some(input_stream)) =
        (tokens.next(), tokens.next(), tokens.next())
    else {
        return false;
    };
    output_var.kind() == "identifier"
        && keyword.utf8_text(source) == Ok("in")
        && input_stream.kind() == "identifier"
}

/// True if `value` is `Arc::from_raw(..)` / `Box::from_raw(..)` /
/// `Rc::from_raw(..)` or a bare `from_raw(..)` call — the reconstruct-and-drop
/// idiom used in `RawWakerVTable::drop` and similar destructors.
fn is_from_raw_reconstruction(value: Node, source: &[u8]) -> bool {
    if value.kind() != "call_expression" {
        return false;
    }
    let Some(function) = value.child_by_field_name("function") else {
        return false;
    };
    let Ok(callee) = function.utf8_text(source) else {
        return false;
    };
    let name = callee.rsplit("::").next().unwrap_or(callee);
    name == "from_raw"
}

/// True if `value` is a turbofish call whose type arguments fix the error type
/// to `Infallible` (`let _ = f::<Infallible, _>(..)` /
/// `f::<std::convert::Infallible, _>(..)`). `Result<_, Infallible>` has an
/// uninhabited `Err` side, so the result can never be `Err` — discarding it
/// ignores no error.
///
/// Matched purely on the turbofish: the `call_expression`'s function must be a
/// `generic_function` carrying `type_arguments`, and at least one type argument
/// must have a final path segment of `Infallible` (matching `Infallible`,
/// `std::convert::Infallible`, and `core::convert::Infallible` via
/// `rsplit("::")`). The inferred `_` placeholder and a real error type
/// (`f::<MyError, _>(..)`) do not match, so those still fire.
fn has_infallible_turbofish(value: Node, source: &[u8]) -> bool {
    if value.kind() != "call_expression" {
        return false;
    }
    let Some(function) = value.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "generic_function" {
        return false;
    }
    let Some(type_arguments) = function.child_by_field_name("type_arguments") else {
        return false;
    };
    let mut cursor = type_arguments.walk();
    type_arguments.named_children(&mut cursor).any(|arg| {
        let Ok(text) = arg.utf8_text(source) else {
            return false;
        };
        text.rsplit("::").next().unwrap_or(text).trim() == "Infallible"
    })
}

/// True if some operation reachable from `value` produces an `Err` its caller
/// cannot act on — a channel send, a standard-stream write, a `core::fmt::Write`
/// call, or an in-memory `write!` — so `let _ =` discards nothing handleable.
///
/// Each producer predicate describes the call that *builds* the error, and that
/// call is not always the outermost one. Two links are followed here:
/// - the receiver of a method call, whose error the outer call forwards
///   (`let _ = tx.send(x).map_err(Box::new)`);
/// - the result of a closure the call runs, since a combinator that reports its
///   closure's failure returns that closure's error
///   (`let _ = iter.try_for_each(|x| tx.send(x))`).
///
/// Any closure-taking call qualifies — no combinator name is matched — and a
/// turbofish call (`collect::<Result<_, _>>()`) is unwrapped to reach its
/// receiver. Call arguments other than closures are not entered, so
/// `let _ = wrap(tx.send(x))` still fires: `wrap`'s own `Err` is unknown. An
/// awaited producer is not reached either, so `let _ = tx.send(x).await` still
/// fires.
///
/// One reachable ignorable producer is enough, so three shapes are accepted
/// false negatives — separating the `Err` types needs the type information this
/// rule does not have:
/// - a chain that also carries a genuinely fallible step
///   (`let _ = tx.send(x).and_then(|_| fs::write(p, d))`);
/// - a combinator whose closure feeds the `Ok` side while the discarded `Err` is
///   the receiver's (`let _ = fs::read(p).map(|d| tx.send(d))`);
/// - a call that stores or hands off the closure instead of reporting its
///   failure, so the discarded `Err` is the call's own
///   (`let _ = pool.execute(move || tx.send(x))`).
///
/// A producer predicate's own imprecision travels along these links too: since
/// `is_channel_send` matches any `.send(..)`, an HTTP client send inside a
/// closure (`let _ = urls.iter().try_for_each(|u| client.send(u))`) is exempt
/// like the outermost `let _ = client.send(u)` already is.
fn has_unhandleable_error_origin(value: Node, source: &[u8]) -> bool {
    if is_channel_send(value, source)
        || is_std_stream_write(value, source)
        || is_fmt_write_method(value, source)
        || is_fmt_write_macro_to_buffer(value, source)
    {
        return true;
    }
    if value.kind() != "call_expression" {
        return false;
    }
    // `expr.collect::<Result<_, _>>()` — the turbofish wraps the method access.
    let callee = value
        .child_by_field_name("function")
        .and_then(|function| match function.kind() {
            "generic_function" => function.child_by_field_name("function"),
            _ => Some(function),
        });
    if let Some(callee) = callee
        && callee.kind() == "field_expression"
        && let Some(receiver) = callee.child_by_field_name("value")
        && has_unhandleable_error_origin(receiver, source)
    {
        return true;
    }
    let Some(arguments) = value.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = arguments.walk();
    arguments.named_children(&mut cursor).any(|argument| {
        argument.kind() == "closure_expression"
            && closure_tail_expression(argument)
                .is_some_and(|tail| has_unhandleable_error_origin(tail, source))
    })
}

/// The node a closure evaluates to: its body when the body is a single
/// expression, or the tail expression of a block body. Only that tail is
/// inspected, so a producer reached through `?`, an explicit `return`, or a
/// branch tail is not seen and such a discard still fires. Comments are named
/// nodes in the grammar, so trailing ones are skipped: a comment after the tail
/// must not hide it.
fn closure_tail_expression(closure: Node<'_>) -> Option<Node<'_>> {
    let body = closure.child_by_field_name("body")?;
    if body.kind() != "block" {
        return Some(body);
    }
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter(|child| !matches!(child.kind(), "line_comment" | "block_comment"))
        .last()
}

/// True if `value` is a method call `expr.send(..)` — the best-effort channel
/// fire-and-forget idiom (`oneshot`/`mpsc` sender). An `Err` from `send` only
/// signals the receiver dropped, which `let _ =` intentionally ignores on a
/// shutdown/cleanup path.
fn is_channel_send(value: Node, source: &[u8]) -> bool {
    if value.kind() != "call_expression" {
        return false;
    }
    let Some(function) = value.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    let Some(field) = function.child_by_field_name("field") else {
        return false;
    };
    matches!(field.utf8_text(source), Ok("send"))
}

/// Std-collection method names whose return type is `Option`/`bool`/`()` and
/// never `Result`, so discarding the value with `let _ =` ignores no error:
/// - `remove`     — `HashMap`/`BTreeMap` → `Option<V>`, sets → `bool`, `Vec` → `T`
/// - `remove_entry`— `HashMap`/`BTreeMap` → `Option<(K, V)>`
/// - `insert`     — maps → `Option<V>`, sets → `bool`
/// - `pop`        — `Vec`/`VecDeque`/`String` → `Option<_>`
/// - `pop_front` / `pop_back` — `VecDeque` → `Option<_>`
/// - `push`       — `Vec`/`String`/`VecDeque` → `()`
/// - `take`       — `Option`/`Cell`/`mem::take` → the owned value
/// - `replace`    — `Option`/`Cell`/`mem::replace`/`str` → the prior/new value
///
/// Curated and tight on purpose: ambiguous names that commonly return `Result`
/// in std or the wider ecosystem (`read`, `write`, `next`, `recv`, `get`,
/// `get_mut`) are deliberately excluded — exempting them would mask genuinely
/// ignored errors. Worst case for a name listed here is a benign false-negative
/// (a same-named user method that does return `Result` goes unflagged), never a
/// new false positive: a conservative lint should err toward under-flagging.
const NON_RESULT_METHODS: &[&str] = &[
    "remove",
    "remove_entry",
    "insert",
    "pop",
    "pop_front",
    "pop_back",
    "push",
    "take",
    "replace",
];

/// True if `value` is a method call `expr.<method>(..)` whose `<method>` is in
/// the curated [`NON_RESULT_METHODS`] set — a std-collection method that
/// returns `Option`/`bool`/`()` and never `Result`.
///
/// Matched on the method-call shape (`call_expression` → `field_expression`
/// `field`), so it only exempts a method invoked on a receiver. The free
/// function `let _ = std::fs::remove_file(p)` — which returns `io::Result`
/// despite sharing the `remove*` stem — is a `call_expression` whose function
/// is a `scoped_identifier`, not a `field_expression`, so it still fires.
fn is_non_result_std_method(value: Node, source: &[u8]) -> bool {
    if value.kind() != "call_expression" {
        return false;
    }
    let Some(function) = value.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    let Some(field) = function.child_by_field_name("field") else {
        return false;
    };
    let Ok(name) = field.utf8_text(source) else {
        return false;
    };
    NON_RESULT_METHODS.contains(&name)
}

/// True if `value` is a `core::fmt::Write` method call (`write_str`/
/// `write_char`). These names are specific to `fmt::Write`; on an in-memory
/// buffer (`String`, `fmt::Formatter`, `Vec<u8>` wrapper) the returned
/// `fmt::Result` is structurally `Ok(())`, so `let _ =` drops an always-Ok
/// unit rather than ignoring an error. Matching by method name keeps the
/// exemption type-safe without type inference, since `io::Write`'s fallible
/// methods (`write`/`write_all`) use different names.
fn is_fmt_write_method(value: Node, source: &[u8]) -> bool {
    if value.kind() != "call_expression" {
        return false;
    }
    let Some(function) = value.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    let Some(field) = function.child_by_field_name("field") else {
        return false;
    };
    matches!(field.utf8_text(source), Ok("write_str" | "write_char"))
}

/// True if `value`'s terminal (outermost) method is `Result::ok`/`Result::err`
/// (`let _ = expr.ok()` / `expr.err()`). These std combinators convert a
/// `Result<T, E>` into an `Option<T>`/`Option<E>`, structurally discarding the
/// opposite arm — their return type is `Option`, never `Result` — so once the
/// terminal `.ok()`/`.err()` has stripped the error, the value bound by
/// `let _ =` carries none to handle. Keyed on the terminal method name (the
/// outermost `call_expression` → `field_expression` `field`), the same
/// mechanism as `write_str`/`write_char`, so a bare `let _ = fallible()` (a
/// `call_expression` whose function is an `identifier`, not a `field_expression`)
/// or a chain whose terminal call returns a `Result` (`let _ = expr.lookup()`)
/// still fires.
fn is_result_to_option_combinator(value: Node, source: &[u8]) -> bool {
    if value.kind() != "call_expression" {
        return false;
    }
    let Some(function) = value.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    let Some(field) = function.child_by_field_name("field") else {
        return false;
    };
    matches!(field.utf8_text(source), Ok("ok" | "err"))
}

/// True if `value` is a `write!`/`writeln!` macro whose writer (first) argument
/// provably resolves to an in-memory buffer — a `String`, `fmt::Formatter`, or
/// `Vec<u8>`. Writing to such a buffer yields a `Result` that is structurally
/// `Ok(())` (an in-memory write cannot fail), so `let _ =` drops an always-Ok
/// unit rather than ignoring an error — the macro sibling of the
/// `write_str`/`write_char` method exemption.
///
/// The writer is the first comma-delimited macro argument; it is resolved only
/// when it is a bare identifier, from the enclosing scope: an annotation naming a
/// buffer type (via [`find_identifier_type`]), or the expression the writer's
/// binding takes its value from (via
/// [`local_binding_init_expression`][crate::rules::rust_helpers::local_binding_init_expression])
/// when that expression yields a `String`/`Vec`. A fallible `io::Write` writer
/// (`File`, `BufWriter`, socket), a non-identifier writer (`self.buf`), or an
/// unresolvable writer is not matched, so `let _ = writeln!(file, ..)` still
/// fires.
fn is_fmt_write_macro_to_buffer(value: Node, source: &[u8]) -> bool {
    if value.kind() != "macro_invocation"
        || !matches!(
            macro_last_name_segment(value, source),
            Some("write" | "writeln")
        )
    {
        return false;
    }
    let Some(writer) = macro_token_tree(value).and_then(macro_first_arg_identifier) else {
        return false;
    };
    let Ok(name) = writer.utf8_text(source) else {
        return false;
    };
    // An annotation resolves the type directly; otherwise the expression the
    // writer's binding takes its value from settles what the writer holds.
    if let Some(ty) = find_identifier_type(writer, name, source)
        && is_in_memory_writer_type(&ty)
    {
        return true;
    }
    local_binding_init_expression(writer, name, source)
        .is_some_and(|origin| expression_yields_buffer(origin, source))
}

/// The last segment of a macro invocation's name — `writeln` for both `writeln!`
/// and a qualified `std::writeln!`.
fn macro_last_name_segment<'a>(value: Node, source: &'a [u8]) -> Option<&'a str> {
    let name = value.child_by_field_name("macro")?.utf8_text(source).ok()?;
    Some(name.rsplit("::").next().unwrap_or(name))
}

/// A macro invocation's token tree — an unnamed child, so no field name reaches
/// it.
fn macro_token_tree(value: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = value.walk();
    value
        .children(&mut cursor)
        .find(|child| child.kind() == "token_tree")
}

/// The writer of a `write!`/`writeln!` `token_tree` when it is a single bare
/// identifier. tree-sitter parses the macro body as an opaque token stream, so
/// the writer appears as the tokens up to the first top-level `,`. Returns that
/// identifier node only when the segment is exactly one `identifier` token — a
/// `self.buf`/`x.y` writer spans several tokens and is deliberately not resolved
/// here (it stays flagged). Only the token tree's direct children are read, so a
/// comma nested in a deeper `token_tree` does not end the writer segment early.
fn macro_first_arg_identifier(token_tree: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = token_tree.walk();
    let mut writer: Option<Node<'_>> = None;
    let mut token_count = 0usize;
    for child in token_tree.children(&mut cursor) {
        match child.kind() {
            "(" | ")" | "[" | "]" | "{" | "}" => {}
            "," => break,
            "identifier" => {
                token_count += 1;
                writer = Some(child);
            }
            _ => token_count += 1,
        }
    }
    if token_count == 1 { writer } else { None }
}

/// True if `ty` (a resolved binding/parameter type's source text) names an
/// in-memory write buffer whose `write!`/`writeln!` result is structurally
/// `Ok(())`: `String`, `fmt::Formatter`, or `Vec<u8>`, seen through any leading
/// `&`/`&mut` and trailing generic (`<'_>`) args. A fallible `io::Write` writer
/// (`File`, `BufWriter`, `TcpStream`) does not match.
fn is_in_memory_writer_type(ty: &str) -> bool {
    let base = ty.trim().trim_start_matches('&').trim_start();
    let base = base.strip_prefix("mut ").unwrap_or(base).trim_start();
    if base.replace(' ', "") == "Vec<u8>" {
        return true;
    }
    let head = base.split('<').next().unwrap_or(base).trim();
    let last = head.rsplit("::").next().unwrap_or(head).trim();
    matches!(last, "String" | "Formatter")
}

/// True if any method in the `value` call chain is `.map_err(<closure>)` or
/// `.inspect_err(<closure>)` — the error has been explicitly observed/handled
/// (e.g. logged) inside the closure before the resulting `Result` is discarded.
/// `let _ = expr.map_err(|e| log::error!(..))` is therefore not an ignored
/// exception: it drops an already-handled result.
///
/// The closure argument is required so that a `map_err`/`inspect_err` taking a
/// bare function reference (which may itself silently swallow the error) is not
/// exempted. The chain is walked through `await`/method links so the method can
/// sit anywhere in the chain (e.g. `x().await.map_err(..)`), not just outermost.
fn chain_has_error_handling_closure(value: Node, source: &[u8]) -> bool {
    let mut node = value;
    loop {
        match node.kind() {
            "await_expression" => {
                let Some(inner) = node.named_child(0) else {
                    return false;
                };
                node = inner;
            }
            "call_expression" => {
                let Some(function) = node.child_by_field_name("function") else {
                    return false;
                };
                if function.kind() == "field_expression"
                    && let Some(field) = function.child_by_field_name("field")
                    && matches!(field.utf8_text(source), Ok("map_err" | "inspect_err"))
                    && call_has_closure_argument(node)
                {
                    return true;
                }
                // Descend into the receiver of this call/method.
                node = match function.kind() {
                    "field_expression" => {
                        let Some(receiver) = function.child_by_field_name("value") else {
                            return false;
                        };
                        receiver
                    }
                    _ => return false,
                };
            }
            "field_expression" => {
                let Some(receiver) = node.child_by_field_name("value") else {
                    return false;
                };
                node = receiver;
            }
            _ => return false,
        }
    }
}

/// True if the `call_expression`'s argument list contains a closure — the
/// signal that the error is observed inside the handler rather than discarded.
fn call_has_closure_argument(call: Node) -> bool {
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = arguments.walk();
    arguments
        .named_children(&mut cursor)
        .any(|arg| arg.kind() == "closure_expression")
}

/// True if `value` is a best-effort write to a standard stream — the error has
/// nowhere to be propagated in an error-reporting path, so `let _ =` drops it
/// intentionally. Recognizes two spellings:
///
/// - Method form `stderr()/stdout().write_all(..)` (also `write`/`write_fmt`/
///   `flush`): a write method whose receiver chain roots at `stderr()`/
///   `stdout()`. Anchoring on the root receiver (not just the method name) keeps
///   it tight: `let _ = file.write_all(..)` to an ordinary writer still fires.
/// - Macro form: `print!`/`println!`/`eprint!`/`eprintln!` target a standard
///   stream by definition, so they are unconditionally exempt; `write!`/
///   `writeln!` only when the writer (their first argument) roots at
///   `stderr()`/`stdout()`, so `let _ = writeln!(file, ..)` still fires.
fn is_std_stream_write(value: Node, source: &[u8]) -> bool {
    match value.kind() {
        "call_expression" => is_std_stream_write_method(value, source),
        "macro_invocation" => is_std_stream_write_macro(value, source),
        _ => false,
    }
}

/// Method form: `stderr()/stdout().write_all(..)` (also `write`/`write_fmt`/
/// `flush`) whose receiver chain roots at a standard stream.
fn is_std_stream_write_method(value: Node, source: &[u8]) -> bool {
    let Some(function) = value.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    let Some(field) = function.child_by_field_name("field") else {
        return false;
    };
    let is_write_method = matches!(
        field.utf8_text(source),
        Ok("write_all" | "write" | "write_fmt" | "flush")
    );
    if !is_write_method {
        return false;
    }
    let Some(receiver) = function.child_by_field_name("value") else {
        return false;
    };
    receiver_roots_at_std_stream(receiver, source)
}

/// Macro form: `print!`/`println!`/`eprint!`/`eprintln!` are unconditionally a
/// std-stream write; `write!`/`writeln!` only when their first token-tree
/// argument (the writer) roots at a `stderr()`/`stdout()` call. The writer is
/// either spelled inline, and the token stream heads at the call, or it is a bare
/// identifier, which [`receiver_roots_at_std_stream`] resolves the same way it
/// resolves a method call's receiver.
fn is_std_stream_write_macro(value: Node, source: &[u8]) -> bool {
    let Some(name) = macro_last_name_segment(value, source) else {
        return false;
    };
    match name {
        "print" | "println" | "eprint" | "eprintln" => true,
        "write" | "writeln" => macro_token_tree(value).is_some_and(|token_tree| {
            macro_first_arg_heads_at_std_stream(token_tree, source)
                || macro_first_arg_identifier(token_tree)
                    .is_some_and(|writer| receiver_roots_at_std_stream(writer, source))
        }),
        _ => false,
    }
}

/// True if the first argument of a `write!`/`writeln!` `token_tree` heads at a
/// `stderr()`/`stdout()` call. tree-sitter parses macro bodies as opaque token
/// streams, so the writer `std::io::stdout()` appears as a run of token nodes
/// (`std`, `::`, `io`, `::`, `stdout`, then a `token_tree` `()`), not a parsed
/// `call_expression`. The writer is the first comma-delimited segment; we scan
/// the token_tree's direct children up to the first top-level `,` for an
/// `identifier` named `stderr`/`stdout` immediately followed by a `token_tree`
/// (the call parens) — matching `stdout()`, `io::stdout()`, `std::io::stdout()`
/// (the `::` separators never sit between the `stdout` identifier and its call
/// parens). Iterating all children (not just named ones) is required because
/// the comma boundary is an anonymous node; only direct children are scanned,
/// so a comma nested in a deeper `token_tree` does not end the writer segment
/// early. Inspecting token node kinds/text (not a raw-source substring search)
/// keeps the writer arg anchored at an actual call.
fn macro_first_arg_heads_at_std_stream(token_tree: Node, source: &[u8]) -> bool {
    let mut cursor = token_tree.walk();
    let mut prev_is_std_stream_ident = false;
    for child in token_tree.children(&mut cursor) {
        match child.kind() {
            // First top-level comma ends the writer argument.
            "," => break,
            // The call parens following a `stderr`/`stdout` identifier.
            "token_tree" if prev_is_std_stream_ident => return true,
            "identifier" => {
                prev_is_std_stream_ident =
                    matches!(child.utf8_text(source), Ok("stderr" | "stdout"));
            }
            _ => prev_is_std_stream_ident = false,
        }
    }
    false
}

/// True if the receiver chain bottoms out at a `stderr()`/`stdout()` call,
/// walking through any intermediate method calls (`stderr().lock()`) and through
/// the bindings that name the handle (`let out = io::stdout(); let mut lock =
/// out.lock();`). Resolving a named handle is what keeps the two spellings of one
/// write in agreement: `writeln!(lock, ..)` and `lock.write_all(..)` reach the
/// same root. What the binding is called says nothing — only the function the
/// chain bottoms out in does, read as the callee's last path segment, so a
/// re-export of the same handle (`anstream::stdout()`) roots it too.
fn receiver_roots_at_std_stream(mut node: Node, source: &[u8]) -> bool {
    for _ in 0..MAX_RECEIVER_HOPS {
        match node.kind() {
            // A named handle — continue from the expression its binding holds.
            "identifier" => {
                let Ok(name) = node.utf8_text(source) else {
                    return false;
                };
                let Some(origin) = local_binding_init_expression(node, name, source) else {
                    return false;
                };
                node = origin;
            }
            "call_expression" => {
                let Some(function) = node.child_by_field_name("function") else {
                    return false;
                };
                match function.kind() {
                    // `stderr()` / `std::io::stderr()` — the chain root.
                    "identifier" | "scoped_identifier" => {
                        let Ok(callee) = function.utf8_text(source) else {
                            return false;
                        };
                        let name = callee.rsplit("::").next().unwrap_or(callee);
                        return name == "stderr" || name == "stdout";
                    }
                    // `stderr().lock()` etc. — descend into the receiver.
                    "field_expression" => {
                        let Some(receiver) = function.child_by_field_name("value") else {
                            return false;
                        };
                        node = receiver;
                    }
                    _ => return false,
                }
            }
            _ => return false,
        }
    }
    false
}

/// How many links [`receiver_roots_at_std_stream`] follows. A named handle costs
/// two: one to resolve the name, one to descend into the receiver of the call it
/// holds. Each link moves to a strictly smaller receiver or to a binding declared
/// strictly earlier, so a chain ends on its own; the bound keeps the walk linear
/// on a file that stacks hundreds of links, at the price of leaving a deeper
/// chain unresolved.
const MAX_RECEIVER_HOPS: usize = 8;

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

    /// Run with a `FileCtx` built from `path` so path-derived flags
    /// (`in_benchmark_dir`) are populated — the bare `run_rule` helper supplies
    /// the empty static `FileCtx`, where those flags are always false.
    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        let path = std::path::Path::new(path);
        let project = crate::project::default_static_project_ctx();
        let file = crate::rules::file_ctx::FileCtx::build(
            path,
            source,
            crate::files::Language::Rust,
            project,
        );
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, path, project, &file)
    }

    #[test]
    fn flags_let_underscore_call() {
        let src = "fn f() { let _ = do_something(); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_let_underscore_macro() {
        let src = "fn f() { let _ = try_parse!(input); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_named_binding() {
        let src = "fn f() { let _result = do_something(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_let_underscore_literal() {
        let src = "fn f() { let _ = 42; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_let_underscore_call_inside_test_function() {
        // The user's reported FP family — a `#[test]` fn where
        // `let _ = …` asserts "no panic" without consuming the value.
        let src = r#"
            #[test]
            fn missing_config_falls_back_to_defaults() {
                let cfg = Config::load_from(tmp.path()).unwrap();
                let _ = cfg.threshold("max-function-lines", "max");
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_let_underscore_call_inside_cfg_test_module() {
        let src = r#"
            #[cfg(test)]
            mod tests {
                fn helper() {
                    let _ = do_something();
                }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_let_underscore_call_inside_kani_proof_harness() {
        // hifitime's `src/epoch/kani_verif.rs`: a Kani harness in the
        // production `src/` tree where `let _ = f(kani::any())` exercises the
        // call for its safety property and discards the symbolic result.
        let proof = r#"
            #[kani::proof]
            fn kani_harness_january_years() {
                let year: i32 = kani::any();
                let _ = january_years(year);
            }
        "#;
        let for_contract = r#"
            #[kani::proof_for_contract(crate::epoch::Epoch::weekday)]
            fn kani_harness_weekday() {
                let dur: Duration = kani::any();
                let callee = Epoch::from_duration(dur, TimeScale::TAI);
                let _ = callee.weekday();
            }
        "#;
        assert!(run_on(proof).is_empty());
        assert!(run_on(for_contract).is_empty());
    }

    #[test]
    fn flags_let_underscore_call_outside_kani_proof() {
        // A plain production fn is still checked even when the file happens to
        // contain Kani harnesses — the attribute, not the file, scopes the
        // exemption.
        let src = "fn produce() { let _ = persist_to_disk(record); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_let_underscore_call_inside_tokio_test() {
        let src = r#"
            #[tokio::test]
            async fn test_send_side_effect() {
                let _ = tx.send(item).await;
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_let_underscore_call_inside_actix_test() {
        let src = r#"
            #[actix_rt::test]
            async fn test_cleanup() {
                let _ = handle.abort();
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_let_underscore_from_raw_intentional_drop() {
        // Regression for #1408: reconstructing an owning pointer to run its
        // Drop is infallible, not an ignored error.
        let arc = "unsafe fn drop_waker(raw: *const ()) { let _ = Arc::from_raw(raw); }";
        let boxed = "unsafe fn drop_box(raw: *const ()) { let _ = Box::from_raw(raw); }";
        let bare = "unsafe fn drop_waker(raw: *const ()) { let _ = from_raw(raw); }";
        assert!(run_on(arc).is_empty());
        assert!(run_on(boxed).is_empty());
        assert!(run_on(bare).is_empty());
    }

    #[test]
    fn allows_let_underscore_try_expression() {
        // Regression for #1410: `?` propagates the error, so `let _ =` only
        // discards the unwrapped success value — the error is handled.
        let method = "fn f() -> Result<()> { let _ = parser.expect(T![NAMESPACE])?; Ok(()) }";
        let call = "fn f() -> Result<()> { let _ = fallible()?; Ok(()) }";
        assert!(run_on(method).is_empty());
        assert!(run_on(call).is_empty());
    }

    #[test]
    fn flags_let_underscore_call_without_question_mark() {
        // The boundary of #1410: without `?`, the Result is genuinely
        // swallowed and must still fire.
        let src = "fn f() { let _ = fallible(); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_let_underscore_channel_send() {
        // Regression for #2007: `let _ = sender.send(..)` on a oneshot/mpsc
        // sender is the best-effort fire-and-forget idiom — an `Err` only
        // means the receiver dropped on a shutdown/cleanup path.
        let resp = "fn f() { let _ = resp.send(Ok(candidates)); }";
        let tx = "fn f() { let _ = tx.send(Err(e)); }";
        assert!(run_on(resp).is_empty());
        assert!(run_on(tx).is_empty());
    }

    #[test]
    fn flags_let_underscore_non_send_method() {
        // The send exemption is scoped to `send`: any other discarded method
        // call result is still genuinely swallowed.
        let src = "fn f() { let _ = foo.bar(); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_let_underscore_channel_send_reached_through_the_expression() {
        // Regression for #6867: starship's directory walker discards the result
        // of a combinator whose only fallible step is the channel send. The
        // `Err` is the sender's, so the discard is the same fire-and-forget as
        // `let _ = tx.send(x)` — wherever the send sits in the expression.
        let starship = r#"
            fn walk(base: PathBuf, tx: Sender<DirEntry>) {
                let dir_iter = fs::read_dir(base).unwrap();
                let _ = dir_iter
                    .filter_map(|entry| entry.ok())
                    .try_for_each(|entry| tx.send(entry).map_err(Box::new));
            }
        "#;
        let forwarded = "fn f() { let _ = tx.send(x).map_err(Box::new); }";
        let and_then = "fn f() { let _ = ready.and_then(|x| tx.send(x)); }";
        let collected =
            "fn f() { let _ = items.iter().map(|x| tx.send(x)).collect::<Result<(), _>>(); }";
        let block_body =
            "fn f() { let _ = items.try_for_each(|i| { let v = i.value(); tx.send(v) }); }";
        // Comments are named nodes, so a trailing one must not hide the value
        // the block evaluates to.
        let commented_block_body =
            "fn f() { let _ = items.try_for_each(|i| { tx.send(*i) // forward it\n }); }";
        assert!(run_on(starship).is_empty());
        assert!(run_on(forwarded).is_empty());
        assert!(run_on(and_then).is_empty());
        assert!(run_on(collected).is_empty());
        assert!(run_on(block_body).is_empty());
        assert!(run_on(commented_block_body).is_empty());
    }

    #[test]
    fn allows_let_underscore_write_reached_through_the_expression() {
        // The standard-stream and `core::fmt::Write` producers travel the same
        // links as the channel send, so a combinator wrapping one of them is
        // exempt too.
        let to_stdout = r#"fn f() { let _ = lines.iter().try_for_each(|l| writeln!(std::io::stdout(), "{l}")); }"#;
        let to_formatter =
            "fn f(f: &mut Formatter) { let _ = parts.iter().try_for_each(|p| f.write_str(p)); }";
        let to_buffer = r#"fn f() { let mut s = String::new(); let _ = parts.iter().try_for_each(|p| write!(s, "{p}")); }"#;
        assert!(run_on(to_stdout).is_empty());
        assert!(run_on(to_formatter).is_empty());
        assert!(run_on(to_buffer).is_empty());
    }

    #[test]
    fn flags_let_underscore_fallible_call_reached_through_the_expression() {
        // Negative space for #6867: the exemption follows the error's producer,
        // not the combinator. A closure whose result is a genuinely fallible
        // call still fires, and so does a closure body that returns nothing.
        let removal = "fn f() { let _ = paths.iter().try_for_each(|p| fs::remove_file(p)); }";
        let to_file = r#"fn f(file: File) { let _ = lines.iter().try_for_each(|l| writeln!(file, "{l}")); }"#;
        let statement_body = "fn f() { let _ = items.try_for_each(|i| { persist(i); }); }";
        let empty_body = "fn f() { let _ = items.iter().try_for_each(|_i| {}); }";
        let no_closure = "fn f() { let _ = items.iter().try_for_each(persist); }";
        // The boundary the whole search rests on: only closure arguments are
        // entered. A producer passed as an ordinary argument says nothing about
        // the surrounding call's own `Err`.
        let plain_argument = "fn f() { let _ = wrap(tx.send(x)); }";
        assert_eq!(run_on(removal).len(), 1);
        assert_eq!(run_on(to_file).len(), 1);
        assert_eq!(run_on(statement_body).len(), 1);
        assert_eq!(run_on(empty_body).len(), 1);
        assert_eq!(run_on(no_closure).len(), 1);
        assert_eq!(run_on(plain_argument).len(), 1);
    }

    #[test]
    fn allows_let_underscore_std_stream_write() {
        // Regression for #1524: a best-effort write to a standard stream in an
        // error-reporting path. The I/O error has nowhere to be propagated.
        let scoped = "fn p(m: &str) { let _ = std::io::stderr().write_all(m.as_bytes()); }";
        let bare = r#"fn p() { let _ = stderr().write_all(b"\n"); }"#;
        let stdout = r#"fn p() { let _ = std::io::stdout().write_all(b"x"); }"#;
        let flush = "fn p() { let _ = std::io::stderr().flush(); }";
        let locked = r#"fn p() { let _ = std::io::stderr().lock().write_all(b"x"); }"#;
        assert!(run_on(scoped).is_empty());
        assert!(run_on(bare).is_empty());
        assert!(run_on(stdout).is_empty());
        assert!(run_on(flush).is_empty());
        assert!(run_on(locked).is_empty());
    }

    #[test]
    fn flags_let_underscore_non_std_stream_write() {
        // Negative space for #1524: the exemption is anchored on a `stderr()`/
        // `stdout()` root receiver. A `write_all` on an ordinary writer (file,
        // socket, buffer) genuinely swallows the error and must still fire.
        let file = "fn f(mut w: File) { let _ = w.write_all(b\"x\"); }";
        let business = "fn f() { let _ = persist_to_disk(record); }";
        assert_eq!(run_on(file).len(), 1);
        assert_eq!(run_on(business).len(), 1);
    }

    #[test]
    fn allows_let_underscore_std_stream_write_macro() {
        // Regression for #3997: the macro spelling of the best-effort std-stream
        // write. `print!`/`println!`/`eprint!`/`eprintln!` target a standard
        // stream by definition; `write!`/`writeln!` when their first argument
        // (the writer) roots at `stderr()`/`stdout()`. The issue's exact nushell
        // `report_error.rs` fallback is `writeln!(std::io::stdout(), ..)`.
        let writeln_stdout = r#"fn p() { let _ = writeln!(std::io::stdout(), "{report}"); }"#;
        let writeln_stderr = r#"fn p() { let _ = writeln!(std::io::stderr(), "x"); }"#;
        let write_stdout = r#"fn p() { let _ = write!(stdout(), "x"); }"#;
        let write_io_stderr = r#"fn p() { let _ = write!(io::stderr(), "x"); }"#;
        let println = r#"fn p() { let _ = println!("x"); }"#;
        let eprintln = r#"fn p() { let _ = eprintln!("x"); }"#;
        let print = r#"fn p() { let _ = print!("x"); }"#;
        let eprint = r#"fn p() { let _ = eprint!("x"); }"#;
        assert!(run_on(writeln_stdout).is_empty());
        assert!(run_on(writeln_stderr).is_empty());
        assert!(run_on(write_stdout).is_empty());
        assert!(run_on(write_io_stderr).is_empty());
        assert!(run_on(println).is_empty());
        assert!(run_on(eprintln).is_empty());
        assert!(run_on(print).is_empty());
        assert!(run_on(eprint).is_empty());
    }

    #[test]
    fn flags_let_underscore_non_std_stream_write_macro() {
        // Negative space for #3997: `write!`/`writeln!` are exempt only when
        // their first argument roots at a std stream (or resolves to an in-memory
        // buffer, see #7200). A write to a fallible `io::Write` writer (a `File`)
        // genuinely swallows the error and must fire. A non-write macro
        // (`some_macro!`, `vec!`) is likewise not exempt.
        let writeln_file = r#"fn f(mut file: File) { let _ = writeln!(file, "{x}"); }"#;
        let other_macro = "fn f() { let _ = some_macro!(x); }";
        assert_eq!(run_on(writeln_file).len(), 1);
        assert_eq!(run_on(other_macro).len(), 1);
    }

    #[test]
    fn allows_let_underscore_write_macro_to_buffer() {
        // Regression for #7200: `write!`/`writeln!` into an in-memory buffer
        // yields a `Result` that is structurally `Ok(())`, so `let _ =` drops an
        // always-Ok unit. The writer resolves to a `String`/`fmt::Formatter`/
        // `Vec<u8>` via a local `let` initializer or a parameter annotation. The
        // issue's exact cargo `features.rs` `format!`-initialized `msg` example.
        let format_init =
            r#"fn f() { let mut msg = format!("x"); let _ = writeln!(msg, "y"); }"#;
        let string_new = r#"fn f() { let mut s = String::new(); let _ = write!(s, "z"); }"#;
        let with_capacity =
            r#"fn f() { let mut s = String::with_capacity(8); let _ = write!(s, "z"); }"#;
        let string_from = r#"fn f() { let mut s = String::from("a"); let _ = write!(s, "z"); }"#;
        let vec_buf = r#"fn f() { let mut v = Vec::new(); let _ = write!(v, "z"); }"#;
        let string_annot = r#"fn f() { let mut s: String = make(); let _ = write!(s, "z"); }"#;
        let formatter_param = r#"
            impl fmt::Display for T {
                fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    let _ = write!(f, "q");
                    Ok(())
                }
            }
        "#;
        let string_param = r#"fn append(msg: &mut String) { let _ = writeln!(msg, "r"); }"#;
        let annotated_closure_param = r#"
            fn f(items: &[&str]) -> String {
                items.iter().fold(String::new(), |mut output: String, b| {
                    let _ = writeln!(output, "{b}");
                    output
                })
            }
        "#;
        assert!(run_on(format_init).is_empty());
        assert!(run_on(string_new).is_empty());
        assert!(run_on(with_capacity).is_empty());
        assert!(run_on(string_from).is_empty());
        assert!(run_on(vec_buf).is_empty());
        assert!(run_on(string_annot).is_empty());
        assert!(run_on(formatter_param).is_empty());
        assert!(run_on(string_param).is_empty());
        assert!(run_on(annotated_closure_param).is_empty());
    }

    #[test]
    fn flags_let_underscore_write_macro_to_fallible_or_unresolved_writer() {
        // Negative space for #7200: the exemption fires only when the writer
        // provably resolves to an in-memory buffer. A fallible `io::Write` writer
        // (a `File`, whose `io::Result` can genuinely fail) must still flag, and
        // an unresolvable writer (no in-scope binding) stays flagged conservatively.
        let file_init =
            r#"fn f() { let mut file = File::create("p").unwrap(); let _ = writeln!(file, "s"); }"#;
        let file_param = r#"fn f(mut file: File) { let _ = writeln!(file, "s"); }"#;
        let unresolved = r#"fn f() { let _ = writeln!(some_unknown_writer, "t"); }"#;
        // A writer that is not a bare identifier spans several macro tokens and
        // names no binding, so nothing resolves it.
        let field_writer = r#"struct S { buf: File } impl S { fn f(&mut self) { let _ = writeln!(self.buf, "s"); } }"#;
        assert_eq!(run_on(file_init).len(), 1);
        assert_eq!(run_on(file_param).len(), 1);
        assert_eq!(run_on(unresolved).len(), 1);
        assert_eq!(run_on(field_writer).len(), 1);
    }

    #[test]
    fn allows_let_underscore_write_macro_to_fold_seeded_accumulator() {
        // Regression for #8364: a seeded fold gives its accumulator the seed's
        // value (see `rust_helpers::fold_seed_for_closure_parameter`), so this is
        // the #7200 premise reached through a closure parameter instead of a
        // `let`. starship's `print.rs:552` shape.
        let fold = r#"
            fn preset_list(items: &[&str]) -> String {
                items.iter().fold(String::new(), |mut output, b| {
                    let _ = writeln!(output, "{b}");
                    output
                })
            }
        "#;
        let try_fold = r#"
            fn f(items: &[&str]) {
                let _r = items.iter().try_fold(String::new(), |mut out, b| {
                    let _ = writeln!(out, "{b}");
                    Ok(out)
                });
            }
        "#;
        let scan = r#"
            fn f(items: &[&str]) {
                let _v: Vec<_> = items.iter().scan(String::new(), |state, b| {
                    let _ = write!(state, "{b}");
                    Some(1)
                }).collect();
            }
        "#;
        assert!(run_on(fold).is_empty());
        assert!(run_on(try_fold).is_empty());
        assert!(run_on(scan).is_empty());
    }

    #[test]
    fn flags_let_underscore_write_macro_to_fallible_fold_seed() {
        // Negative space for #8364: the seed settles what the accumulator holds,
        // so a fallible seed keeps the write flagged. Nor does the seed reach the
        // closure's second parameter (the iterator item), a shadowing `let` inside
        // the closure, or a call whose shape is not one of the three signatures
        // `rust_helpers::fold_seed_for_closure_parameter` documents.
        let file_seed = r#"
            fn f(items: &[&str], p: &str) {
                let _r = items.iter().fold(File::create(p).unwrap(), |mut w, b| {
                    let _ = writeln!(w, "{b}");
                    w
                });
            }
        "#;
        let item_param = r#"
            fn f(items: &[File]) -> String {
                items.iter().fold(String::new(), |out, w| {
                    let _ = writeln!(w, "x");
                    out
                })
            }
        "#;
        let shadowed_in_closure = r#"
            fn f(items: &[&str], p: &str) -> String {
                items.iter().fold(String::new(), |output, b| {
                    let mut output = File::create(p).unwrap();
                    let _ = writeln!(output, "{b}");
                    output
                })
            }
        "#;
        let map_or = r#"
            fn f(x: Option<File>) {
                let _r = x.map_or(String::new(), |mut w| {
                    let _ = writeln!(w, "y");
                    String::new()
                });
            }
        "#;
        // An annotation that contradicts the seed is the stronger statement about
        // the parameter, so the seed does not exempt it.
        let contradicting_annotation = r#"
            fn f(items: &[&str], p: &str) {
                let _r = items.iter().fold(String::new(), |mut w: File, b| {
                    let _ = writeln!(w, "{b}");
                    String::new()
                });
            }
        "#;
        // Three arguments is no signature the seed rule speaks about.
        let extra_argument = r#"
            fn f(xs: &[u8], extra: u8) {
                let _r = xs.scan(String::new(), |o, x| {
                    let _ = write!(o, "{x}");
                    Some(1)
                }, extra);
            }
        "#;
        assert_eq!(run_on(file_seed).len(), 1);
        assert_eq!(run_on(item_param).len(), 1);
        assert_eq!(run_on(shadowed_in_closure).len(), 1);
        assert_eq!(run_on(map_or).len(), 1);
        assert_eq!(run_on(contradicting_annotation).len(), 1);
        assert_eq!(run_on(extra_argument).len(), 1);
    }

    #[test]
    fn allows_let_underscore_write_macro_to_let_bound_std_stream() {
        // Regression for #8364: a `stderr()`/`stdout()` handle held in a binding
        // is the same expression the inline spelling names, so the best-effort
        // std-stream write stays exempt. starship's `main.rs:200` shape.
        let stderr_binding = r#"
            fn note(args: &[String]) {
                let mut stderr = std::io::stderr();
                let _ = writeln!(stderr, "\nNOTE: {args:?}");
            }
        "#;
        let stdout_lock = r#"
            fn f() {
                let mut out = io::stdout().lock();
                let _ = writeln!(out, "x");
            }
        "#;
        // The canonical two-step lock: the handle and its guard each get a
        // binding, so the chain crosses two of them.
        let two_step_lock = r#"
            fn f() {
                let stdout = io::stdout();
                let mut lock = stdout.lock();
                let _ = writeln!(lock, "x");
            }
        "#;
        assert!(run_on(stderr_binding).is_empty());
        assert!(run_on(stdout_lock).is_empty());
        assert!(run_on(two_step_lock).is_empty());
    }

    #[test]
    fn allows_let_underscore_std_stream_write_method_on_let_bound_handle() {
        // Regression for #8364: the method spelling reads the same binding as the
        // macro spelling, so one handle cannot get opposite verdicts depending on
        // how the write is written.
        let write_all = r#"
            fn f() {
                let mut err = io::stderr();
                let _ = err.write_all(b"x");
            }
        "#;
        let flush = r#"
            fn f() {
                let mut out = io::stdout();
                let _ = out.flush();
            }
        "#;
        let two_step_lock = r#"
            fn f() {
                let stdout = io::stdout();
                let mut lock = stdout.lock();
                let _ = lock.write_all(b"x");
            }
        "#;
        assert!(run_on(write_all).is_empty());
        assert!(run_on(flush).is_empty());
        assert!(run_on(two_step_lock).is_empty());
    }

    #[test]
    fn flags_let_underscore_std_stream_write_method_on_let_bound_file() {
        // Negative space for #8364: resolving a method call's receiver through its
        // binding must not exempt an ordinary writer.
        let file_binding = r#"
            fn f(p: &str) {
                let mut w = File::create(p).unwrap();
                let _ = w.write_all(b"x");
            }
        "#;
        assert_eq!(run_on(file_binding).len(), 1);
    }

    #[test]
    fn flags_let_underscore_write_macro_to_let_bound_file() {
        // Negative space for #8364: resolving the writer through its binding must
        // not turn every named writer into a std stream. A `File` handle, and a
        // binding declared in an enclosing `fn` that a nested `fn` cannot see,
        // both stay flagged.
        let file_binding = r#"
            fn f(p: &str) {
                let mut w = File::create(p).unwrap();
                let _ = writeln!(w, "x");
            }
        "#;
        let outer_fn_binding = r#"
            fn outer() {
                let mut w = String::new();
                fn inner(w: &mut File) {
                    let _ = writeln!(w, "x");
                }
            }
        "#;
        // The last binding before the write is the one that answers for it.
        let rebound_to_file = r#"
            fn f(p: &str) {
                let mut w = String::new();
                let mut w = File::create(p).unwrap();
                let _ = writeln!(w, "x");
            }
        "#;
        // A destructuring `let` binds a part of its initializer, which this walk
        // does not follow, so it hides the outer handle rather than answering
        // with it.
        let let_else_destructure = r#"
            fn f(p: &str) {
                let out = io::stdout();
                let Some(mut out) = File::create(p).ok() else { return };
                let _ = writeln!(out, "x");
            }
        "#;
        let tuple_destructure = r#"
            fn f(t: (u8, File)) {
                let out = io::stdout();
                let (n, mut out) = t;
                let _ = out.write_all(b"x");
            }
        "#;
        // A struct field written in shorthand names the binding, so it hides the
        // outer handle like any other destructuring pattern.
        let struct_shorthand = r#"
            fn f(s: S) {
                let out = io::stdout();
                let S { out } = s;
                let _ = writeln!(out, "x");
            }
        "#;
        assert_eq!(run_on(file_binding).len(), 1);
        assert_eq!(run_on(outer_fn_binding).len(), 1);
        assert_eq!(run_on(rebound_to_file).len(), 1);
        assert_eq!(run_on(let_else_destructure).len(), 1);
        assert_eq!(run_on(tuple_destructure).len(), 1);
        assert_eq!(run_on(struct_shorthand).len(), 1);
    }

    #[test]
    fn allows_let_underscore_write_macro_to_reborrowed_buffer() {
        // Regression for #8364: a rebinding that renames or reborrows carries the
        // same value, so the writer is still the `String` the first binding
        // created.
        let reborrow = r#"
            fn f() {
                let mut out = String::new();
                let out = &mut out;
                let _ = writeln!(out, "x");
            }
        "#;
        let moved_into_inner_scope = r#"
            fn f() {
                let mut buf = String::new();
                {
                    let mut buf = buf;
                    let _ = writeln!(buf, "x");
                }
            }
        "#;
        assert!(run_on(reborrow).is_empty());
        assert!(run_on(moved_into_inner_scope).is_empty());
    }

    #[test]
    fn flags_let_underscore_write_macro_to_reborrowed_file() {
        // Negative space for #8364: renaming chains report the value they end at,
        // so a `File` reached through an alias stays flagged. A chain that names
        // itself resolves to nothing rather than looping.
        let file_reborrow = r#"
            fn f(p: &str) {
                let mut w = File::create(p).unwrap();
                let w = &mut w;
                let _ = writeln!(w, "x");
            }
        "#;
        let self_naming_chain = r#"
            fn f() {
                let p = q;
                let q = p;
                let _ = writeln!(q, "x");
            }
        "#;
        assert_eq!(run_on(file_reborrow).len(), 1);
        assert_eq!(run_on(self_naming_chain).len(), 1);
    }

    #[test]
    fn flags_let_underscore_write_past_the_resolution_bound() {
        // Both walks follow a bounded number of links, so a chain longer than the
        // bound leaves the writer unresolved and the write flagged. The shapes a
        // program actually writes are one or two links and stay exempt, as the
        // `reborrowed_buffer` and `let_bound_std_stream` cases show.
        let long_rename_chain = r#"
            fn f() {
                let w0 = String::new();
                let w1 = &mut w0;
                let w2 = &mut w1;
                let w3 = &mut w2;
                let w4 = &mut w3;
                let w5 = &mut w4;
                let w6 = &mut w5;
                let w7 = &mut w6;
                let w8 = &mut w7;
                let w9 = &mut w8;
                let _ = writeln!(w9, "x");
            }
        "#;
        // A named handle costs two links: one to resolve the name, one to descend
        // into the receiver of the call it holds.
        let deep_handle_chain = r#"
            fn f() {
                let a = io::stdout();
                let b = a.lock();
                let c = b.lock();
                let d = c.lock();
                let e = d.lock();
                let _ = e.write_all(b"x");
            }
        "#;
        assert_eq!(run_on(long_rename_chain).len(), 1);
        assert_eq!(run_on(deep_handle_chain).len(), 1);
    }

    #[test]
    fn flags_let_underscore_write_macro_to_shadowing_pattern_binding() {
        // Regression for #8364: a closure parameter, a `for` pattern and an `if
        // let` pattern each shadow an outer binding of the same name, so an outer
        // buffer says nothing about a writer they hand over. Only a seeded fold
        // supplies such a parameter's value.
        let closure_param = r#"
            fn f(files: &[File]) {
                let mut w = String::new();
                files.iter().for_each(|mut w| {
                    let _ = writeln!(w, "x");
                });
            }
        "#;
        let for_pattern = r#"
            fn f(files: Vec<File>) {
                let mut w = String::new();
                for mut w in files {
                    let _ = writeln!(w, "x");
                }
            }
        "#;
        let if_let_pattern = r#"
            fn f(file: Option<File>) {
                let mut w = String::new();
                if let Some(mut w) = file {
                    let _ = writeln!(w, "x");
                }
            }
        "#;
        let while_let_pattern = r#"
            fn f(files: &mut Files) {
                let mut w = String::new();
                while let Some(mut w) = files.next() {
                    let _ = writeln!(w, "x");
                }
            }
        "#;
        let match_pattern = r#"
            fn f(file: File) {
                let mut w = String::new();
                match file {
                    w => { let _ = writeln!(w, "x"); }
                }
            }
        "#;
        // An `if let` guard binds for the arm's body, so it shadows too.
        let match_guard_let_pattern = r#"
            fn f(file: Option<File>) {
                let mut w = String::new();
                match file {
                    _ if let Some(mut w) = file => { let _ = writeln!(w, "x"); }
                    _ => {}
                }
            }
        "#;
        // A struct field written in shorthand names the binding it introduces.
        let struct_shorthand_pattern = r#"
            fn f(entries: Vec<Entry>) {
                let mut w = String::new();
                for Entry { w } in entries {
                    let _ = writeln!(w, "x");
                }
            }
        "#;
        assert_eq!(run_on(closure_param).len(), 1);
        assert_eq!(run_on(for_pattern).len(), 1);
        assert_eq!(run_on(if_let_pattern).len(), 1);
        assert_eq!(run_on(while_let_pattern).len(), 1);
        assert_eq!(run_on(match_pattern).len(), 1);
        assert_eq!(run_on(match_guard_let_pattern).len(), 1);
        assert_eq!(run_on(struct_shorthand_pattern).len(), 1);
    }

    #[test]
    fn allows_let_underscore_write_macro_to_buffer_across_an_unrelated_scope() {
        // Negative space for the shadowing above: only a pattern that binds the
        // writer's own name hides an outer binding, so a `for` loop or an `if let`
        // over another name leaves the buffer resolvable.
        let for_loop = r#"
            fn f(files: Vec<File>) {
                let mut w = String::new();
                for file in files {
                    let _ = write!(w, "{file:?}");
                }
            }
        "#;
        let if_let = r#"
            fn f(file: Option<File>) {
                let mut w = String::new();
                if let Some(f) = file {
                    let _ = write!(w, "{f:?}");
                }
            }
        "#;
        // A guarded arm, with a named pattern and with the wildcard.
        let match_guard = r#"
            fn f(items: &[u8]) {
                let mut w = String::new();
                match items.len() {
                    n if w.is_empty() => { let _ = writeln!(w, "{n}"); }
                    _ => {}
                }
            }
        "#;
        let wildcard_match_guard = r#"
            fn f(items: &[u8]) {
                let mut w = String::new();
                match items.len() {
                    _ if w.is_empty() => { let _ = writeln!(w, "x"); }
                    _ => {}
                }
            }
        "#;
        // A pattern's path names the type it matches against, so a module or
        // variant sharing the writer's name binds nothing.
        let module_path = r#"
            fn f(y: w::T) {
                let mut w = String::new();
                let w::T(n) = y;
                let _ = write!(w, "{n}");
            }
        "#;
        assert!(run_on(for_loop).is_empty());
        assert!(run_on(if_let).is_empty());
        assert!(run_on(match_guard).is_empty());
        assert!(run_on(wildcard_match_guard).is_empty());
        assert!(run_on(module_path).is_empty());
    }

    #[test]
    fn allows_let_underscore_fmt_write_method() {
        // Regression for #1517: `core::fmt::Write` methods on an in-memory
        // buffer return a `fmt::Result` that is always `Ok(())`. The issue's
        // exact axum `sse.rs` examples.
        let write_str = "fn f() { let _ = writer.write_str(data.as_ref()); }";
        let write_char = "fn f() { let _ = event.buffer.as_mut().write_char('\\n'); }";
        let framing = r#"fn f() { let _ = buffer.write_str("data: "); }"#;
        assert!(run_on(write_str).is_empty());
        assert!(run_on(write_char).is_empty());
        assert!(run_on(framing).is_empty());
    }

    #[test]
    fn flags_let_underscore_io_write_method() {
        // Negative space for #1517: `io::Write`'s `write_all` uses a different
        // method name from `fmt::Write` and can return a real I/O error, so a
        // discarded result on an ordinary writer must still fire.
        let write_all = "fn f(mut file: File) { let _ = file.write_all(buf); }";
        assert_eq!(run_on(write_all).len(), 1);
    }

    #[test]
    fn allows_let_underscore_map_err_handling_closure() {
        // Regression for #1457: the error is logged inside the `map_err`/
        // `inspect_err` closure, then the now-trivial Result is discarded.
        // The issue's exact helix `document.rs` examples.
        let remove = "async fn w() { let _ = tokio::fs::remove_file(backup).await.map_err(|e| log::error!(\"Failed to remove backup file on write: {e}\")); }";
        let rename = "async fn w() { let _ = tokio::fs::rename(&backup, &write_path).await.map_err(|e| log::error!(\"Failed to restore backup on write failure: {e}\")); }";
        let block = "async fn w() { let _ = tokio::fs::copy(&backup, &write_path).await.map_err(|e| { delete = false; log::error!(\"Failed to restore backup on write failure: {e}\") }); }";
        let inspect = "async fn w() { let _ = do_io().await.inspect_err(|e| log::error!(\"io failed: {e}\")); }";
        assert!(run_on(remove).is_empty());
        assert!(run_on(rename).is_empty());
        assert!(run_on(block).is_empty());
        assert!(run_on(inspect).is_empty());
    }

    #[test]
    fn flags_let_underscore_without_error_handling_closure() {
        // Negative space for #1457: a discarded fallible call with no
        // `map_err`/`inspect_err` closure still genuinely swallows the error.
        // Also: `map_err` taking a bare function (not a closure) may itself
        // swallow the error, so it must still fire.
        let bare = "fn f() { let _ = fallible(); }";
        let map_fn = "async fn w() { let _ = do_io().await.map_err(handle); }";
        assert_eq!(run_on(bare).len(), 1);
        assert_eq!(run_on(map_fn).len(), 1);
    }

    #[test]
    fn allows_let_underscore_ok_err_combinator() {
        // Regression for #7422: the terminal method is `Result::ok`/`Result::err`,
        // which converts the `Result` into an `Option`, structurally discarding
        // the error arm. The value bound by `let _ =` carries no error to handle.
        // The issue's exact gitui `status.rs` example.
        let ok = "fn update(&mut self) -> Result<()> { let _ = self.git_branch_name.lookup().ok(); Ok(()) }";
        let err = "fn f() { let _ = fallible().err(); }";
        assert!(run_on(ok).is_empty());
        assert!(run_on(err).is_empty());
    }

    #[test]
    fn flags_let_underscore_terminal_result_call() {
        // Negative space for #7422: the exemption is keyed on the TERMINAL method
        // being `.ok()`/`.err()`. A chain whose terminal call returns a `Result`
        // (`lookup()`, no `.ok()`) still genuinely swallows the error, and a bare
        // `let _ = fallible()` (function is an identifier, not a method) fires.
        let terminal_result = "fn h(&self) { let _ = self.thing.lookup(); }";
        let bare = "fn g() { let _ = fallible(); }";
        assert_eq!(run_on(terminal_result).len(), 1);
        assert_eq!(run_on(bare).len(), 1);
    }

    #[test]
    fn allows_let_underscore_in_drop_impl() {
        // Regression for #1459: `Drop::drop` returns `()`, so a fallible result
        // can only be discarded best-effort — there is no way to propagate it.
        // The issue's exact helix `termina.rs` example.
        let src = r#"
            impl Drop for Terminal {
                fn drop(&mut self) {
                    if !std::thread::panicking() {
                        let _ = self.disable_extensions();
                        let _ = self.disable_mouse_capture();
                    }
                }
            }
        "#;
        let qualified = r#"
            impl std::ops::Drop for Terminal {
                fn drop(&mut self) {
                    let _ = self.cleanup();
                }
            }
        "#;
        assert!(run_on(src).is_empty());
        assert!(run_on(qualified).is_empty());
    }

    #[test]
    fn allows_let_underscore_in_async_drop_impl() {
        // Regression for #7805: `AsyncDrop::async_drop` returns `()`, so like
        // `Drop::drop` a fallible result can only be discarded best-effort —
        // there is no `?` or `Result` channel to propagate a cleanup error to.
        // The issue's exact `apache/iggy` example.
        let src = r#"
            #[async_trait]
            impl AsyncDrop for ClientWrapper {
                async fn async_drop(&mut self) {
                    match self {
                        ClientWrapper::Iggy(client) => {
                            let _ = client.logout_user().await;
                        }
                        ClientWrapper::Http(client) => {
                            let _ = client.logout_user().await;
                        }
                    }
                }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_let_underscore_outside_drop_impl() {
        // Negative space for #1459: the exemption is scoped to `fn drop` of a
        // `Drop` impl. A `drop`-named method in an inherent impl, and any other
        // method in a `Drop` impl, can still propagate errors — so a discarded
        // fallible result there genuinely swallows the error and must fire.
        let inherent_drop = r#"
            impl Terminal {
                fn drop(&mut self) { let _ = self.cleanup(); }
            }
        "#;
        let other_method_in_drop_impl = r#"
            impl Drop for Terminal {
                fn helper(&mut self) -> Result<()> { let _ = self.cleanup(); Ok(()) }
            }
        "#;
        assert_eq!(run_on(inherent_drop).len(), 1);
        assert_eq!(run_on(other_method_in_drop_impl).len(), 1);
    }

    #[test]
    fn flags_let_underscore_async_drop_outside_async_drop_impl() {
        // Negative space for #7805: the async-destructor exemption stays paired
        // with `AsyncDrop`. A method named `async_drop` in an inherent impl or
        // under a different trait can still propagate errors, and a method named
        // `drop` under a non-`Drop` trait is unchanged — all still fire.
        let inherent_async_drop = r#"
            impl Cleaner {
                async fn async_drop(&mut self) { let _ = self.flush().await; }
            }
        "#;
        let async_drop_other_trait = r#"
            #[async_trait]
            impl SomethingElse for Cleaner {
                async fn async_drop(&mut self) { let _ = self.flush().await; }
            }
        "#;
        let drop_other_trait = r#"
            impl SomethingElse for Cleaner {
                fn drop(&mut self) { let _ = self.flush(); }
            }
        "#;
        assert_eq!(run_on(inherent_async_drop).len(), 1);
        assert_eq!(run_on(async_drop_other_trait).len(), 1);
        assert_eq!(run_on(drop_other_trait).len(), 1);
    }

    #[test]
    fn allows_let_underscore_non_result_std_method() {
        // Regression for #1458: `let _ = receiver.<method>(..)` where the
        // method is a std collection method returning `Option`/`bool`/`()`
        // (never `Result`) carries no error to ignore. The issue's exact helix
        // `theme.rs` example plus the other non-fallible discards it lists.
        let remove = r#"fn f() { let _ = values.remove("inherits"); }"#;
        let insert_map = "fn f() { let _ = map.insert(key, value); }";
        let insert_set = "fn f() { let _ = set.insert(value); }";
        let pop = "fn f() { let _ = vec.pop(); }";
        let push = "fn f() { let _ = vec.push(value); }";
        let pop_front = "fn f() { let _ = deque.pop_front(); }";
        let take = "fn f() { let _ = self.field.take(); }";
        assert!(run_on(remove).is_empty());
        assert!(run_on(insert_map).is_empty());
        assert!(run_on(insert_set).is_empty());
        assert!(run_on(pop).is_empty());
        assert!(run_on(push).is_empty());
        assert!(run_on(pop_front).is_empty());
        assert!(run_on(take).is_empty());
    }

    #[test]
    fn flags_let_underscore_non_curated_method_and_free_fn() {
        // Negative space for #1458: the exemption is keyed on the method-call
        // SHAPE (a curated method on a receiver). A method NOT in the curated
        // set still fires, and the FREE function `fs::remove_file` (which
        // returns `io::Result`, unlike the `.remove()` METHOD) must still fire
        // even though it shares the `remove_file` stem — it is not a method on
        // a receiver.
        let io_write = "fn f(mut file: File) { let _ = file.write_all(buf); }";
        let parse = "fn f() { let _ = something.parse::<i32>(); }";
        let free_remove_file = "fn f(p: &Path) { let _ = std::fs::remove_file(p); }";
        assert_eq!(run_on(io_write).len(), 1);
        assert_eq!(run_on(parse).len(), 1);
        assert_eq!(run_on(free_remove_file).len(), 1);
    }

    #[test]
    fn allows_let_underscore_in_compile_fail_fixture() {
        // Regression for #1408: compile-fail fixtures use `let _ =` to keep
        // "unused result" warnings out of the expected compiler output.
        let src = "fn f() { let _ = tokio::try_join!(async {}); }";
        let diagnostics = crate::rules::test_helpers::run_rule(
            &Check,
            src,
            "tests-build/tests/fail/macros_try_join.rs",
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn flags_let_underscore_in_ordinary_fail_dir() {
        // The exemption requires a `tests` ancestor; a plain `fail/` dir
        // outside a test harness is still a genuinely ignored result.
        let src = "fn f() { let _ = do_something(); }";
        let diagnostics =
            crate::rules::test_helpers::run_rule(&Check, src, "src/fail/handler.rs");
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn allows_let_underscore_in_tests_dir_helper() {
        // Regression for #3298: a plain `pub fn` (NO `#[test]` attribute) in a
        // file under a `tests/` directory is integration-test infrastructure.
        // The issue's exact ripgrep `tests/util.rs` `link_dir` best-effort
        // cleanup, which the attribute walk alone does not exempt.
        let src = r#"
            pub fn link_dir<S: AsRef<Path>, T: AsRef<Path>>(&self, src: S, target: T) {
                let target = self.dir.join(target);
                let _ = fs::remove_file(&target);
                nice_err(&target, symlink(&src, &target));
            }
        "#;
        let diagnostics = crate::rules::test_helpers::run_rule(&Check, src, "tests/util.rs");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn allows_let_underscore_in_benchmark_file() {
        // Regression for #4925: in a Criterion `b.iter(|| { let _ = f(..) })`
        // closure the discard runs the call for its timing without consuming the
        // result. Benchmark code never ships to production, so it is out of scope
        // like test code. The issue's exact postcard `varint_bench.rs` example.
        let src = r#"
            fn bench_varint_usize_small(c: &mut Criterion) {
                let mut out = [0u8; varint_max::<usize>()];
                c.bench_function("varint_usize_small", |b| {
                    b.iter(|| {
                        let _ = varint_usize(black_box(42), black_box(&mut out));
                    })
                });
            }
        "#;
        let bench_dir = run_on_path(src, "benches/varint_bench.rs");
        let bench_marker = run_on_path(src, "src/varint_bench.rs");
        assert!(bench_dir.is_empty());
        assert!(bench_marker.is_empty());
    }

    #[test]
    fn allows_let_underscore_in_fuzz_target() {
        // Regression for #6417: in a cargo-fuzz harness `let _ = f(fuzzer_input)`
        // exercises a call only to assert it does not panic — success vs. failure
        // is deliberately ignored. The issue's exact proc-macro2
        // `fuzz/fuzz_targets/parse_token_stream.rs` example.
        let src = r#"
            fn do_fuzz(bytes: &[u8]) {
                let Ok(string) = str::from_utf8(bytes) else { return };
                let _ = string.parse::<proc_macro2::TokenStream>();
            }
        "#;
        let fuzz_target = run_on_path(src, "fuzz/fuzz_targets/parse_token_stream.rs");
        assert!(fuzz_target.is_empty());

        // Negative control: the SAME discarded fallible call in production code
        // (not under `fuzz/fuzz_targets/`) genuinely swallows the error and must
        // fire — confirming the fixture path drives the `in_fuzz_targets` flag.
        let prod = run_on_path(src, "src/lib.rs");
        assert_eq!(prod.len(), 1);
    }

    /// Run a rule on `rel_src` written next to `cargo_toml`, so the manifest
    /// (`is_fuzz_crate` exemption) resolves via `nearest_cargo_manifest`.
    fn run_on_with_cargo(cargo_toml: &str, source: &str, rel_src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_cargo(&Check, cargo_toml, source, rel_src)
    }

    const CARGO_FUZZ_CARGO_TOML: &str = r#"
[package]
name = "image-fuzz"
version = "0.0.0"
edition = "2021"

[package.metadata]
cargo-fuzz = true

[dependencies]
libfuzzer-sys = "0.4"
"#;

    const AFL_FUZZ_CARGO_TOML: &str = r#"
[package]
name = "image-fuzz-afl"
version = "0.0.0"
edition = "2021"

[dependencies]
afl = "0.15"
"#;

    const HONGGFUZZ_CARGO_TOML: &str = r#"
[package]
name = "hfuzz-target"
version = "0.0.0"
edition = "2021"

[dependencies]
honggfuzz = "0.5"
"#;

    const NORMAL_LIB_CARGO_TOML: &str = r#"
[package]
name = "normal-lib"
version = "0.1.0"
edition = "2021"

[lib]
name = "normal_lib"
"#;

    #[test]
    fn allows_let_underscore_in_cargo_fuzz_crate() {
        // Closes #7169: a cargo-fuzz crate (`[package.metadata] cargo-fuzz =
        // true`, dep `libfuzzer-sys`) places harnesses under `fuzz/fuzzers/`,
        // NOT `fuzz_targets/`, so the exemption must key on the manifest.
        let src = r#"
            fuzz_target!(|data: &[u8]| {
                let _ = image::load_from_memory(data);
            });
        "#;
        let harness = run_on_with_cargo(CARGO_FUZZ_CARGO_TOML, src, "fuzzers/fuzz_webp.rs");
        assert!(harness.is_empty(), "cargo-fuzz harness discard must not flag");
    }

    #[test]
    fn allows_let_underscore_in_afl_fuzz_crate() {
        // Closes #7169: an AFL crate (dep `afl`) places harnesses under
        // `fuzz-afl/{fuzzers,reproducers}/`. The issue's exact image-rs snippet.
        let src = r#"
            fn main() {
                afl::fuzz(true, |data| {
                    let _ = webp_decode(data);
                });
            }
        "#;
        let fuzzer = run_on_with_cargo(AFL_FUZZ_CARGO_TOML, src, "fuzzers/fuzz_webp.rs");
        let reproducer =
            run_on_with_cargo(AFL_FUZZ_CARGO_TOML, src, "reproducers/reproduce_webp.rs");
        assert!(fuzzer.is_empty(), "AFL fuzzer discard must not flag");
        assert!(reproducer.is_empty(), "AFL reproducer discard must not flag");
    }

    #[test]
    fn flags_let_underscore_in_normal_crate_not_fuzz() {
        // Negative control: the SAME discarded fallible call in a normal crate
        // (no fuzz metadata/deps, no `fuzz_targets/` path) genuinely swallows
        // the error and must fire — the exemption keys on the manifest fact.
        let src = "pub fn cleanup() { let _ = fallible(); }";
        let diagnostics = run_on_with_cargo(NORMAL_LIB_CARGO_TOML, src, "src/lib.rs");
        assert_eq!(diagnostics.len(), 1, "discard in a normal crate must still flag");
    }

    #[test]
    fn manifest_detects_fuzz_crate() {
        // The manifest fact the exemption keys on: `[package.metadata]
        // cargo-fuzz = true` or a fuzzing-runtime dependency parses to
        // `is_fuzz_crate()`; a plain `[lib]` manifest does not.
        use crate::project::CargoManifest;
        use std::path::PathBuf;
        for toml in [CARGO_FUZZ_CARGO_TOML, AFL_FUZZ_CARGO_TOML, HONGGFUZZ_CARGO_TOML] {
            let manifest = CargoManifest::parse(toml, PathBuf::from("/c")).unwrap();
            assert!(manifest.is_fuzz_crate(), "fuzz manifest must parse to is_fuzz_crate");
        }
        let normal = CargoManifest::parse(NORMAL_LIB_CARGO_TOML, PathBuf::from("/c")).unwrap();
        assert!(!normal.is_fuzz_crate(), "a plain lib manifest is not a fuzz crate");
    }

    #[test]
    fn flags_let_underscore_in_non_test_path() {
        // Negative space for #3298: the `tests/` exemption is scoped to test
        // infra. The same discarded fallible call in production code (not under
        // `tests/`, no `#[test]`) genuinely swallows the error and must fire.
        let src = "pub fn cleanup() { let _ = fallible(); }";
        let diagnostics = crate::rules::test_helpers::run_rule(&Check, src, "src/lib.rs");
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn allows_let_underscore_in_process_exit_scope() {
        // Regression for #5734: best-effort terminal cleanup in a Ctrl-C handler
        // closure that calls `process::exit(..)` right after. The process is
        // terminating, so the error cannot be propagated — like `Drop::drop`.
        // The issue's exact cargo-workspaces `main.rs` example.
        let ctrlc = r#"
            fn set_handlers() {
                ctrlc::set_handler(move || {
                    let term = dialoguer::console::Term::stdout();
                    let _ = term.show_cursor();
                    std::process::exit(130);
                })
                .expect("Error setting Ctrl-C handler");
            }
        "#;
        // Bare `process::exit` (a `use std::process;` import) and a discard
        // followed by `exit` in a plain fn body both qualify.
        let unqualified = r#"
            fn shutdown() {
                let _ = restore_terminal();
                process::exit(1);
            }
        "#;
        // The exit may sit in a sibling `if`/match arm in the same block.
        let conditional = r#"
            fn handle() {
                let _ = cleanup();
                if fatal {
                    std::process::exit(2);
                }
            }
        "#;
        assert!(run_on(ctrlc).is_empty());
        assert!(run_on(unqualified).is_empty());
        assert!(run_on(conditional).is_empty());
    }

    #[test]
    fn flags_let_underscore_outside_process_exit_scope() {
        // Negative space for #5734: the exemption requires a `process::exit(..)`
        // sibling call in the same block. An ordinary discard with no exit still
        // fires; a `process::exit` buried in a NESTED closure does not make the
        // outer discard exit-imminent; and a user method `foo.exit()` (not the
        // `process::exit` free function) does not qualify.
        let no_exit = "fn f() { let _ = fallible(); }";
        let nested_closure = r#"
            fn f() {
                let _ = fallible();
                let cb = || std::process::exit(1);
            }
        "#;
        let method_exit = r#"
            fn f(app: App) {
                let _ = fallible();
                app.exit();
            }
        "#;
        assert_eq!(run_on(no_exit).len(), 1);
        assert_eq!(run_on(nested_closure).len(), 1);
        assert_eq!(run_on(method_exit).len(), 1);
    }

    #[test]
    fn allows_let_underscore_infallible_turbofish() {
        // Regression for #3962: a turbofish fixing the error type to
        // `Infallible` yields a `Result<_, Infallible>` whose `Err` side is
        // uninhabited — the result can never be `Err`, so `let _ =` ignores no
        // error. The issue's exact async-graphql `context.rs` example, plus the
        // bare and `core::convert::` spellings.
        let async_graphql =
            "fn f() { let _ = self.try_for_each::<std::convert::Infallible, _>(|x| Ok(())); }";
        let bare = "fn f() { let _ = g::<Infallible, _>(x); }";
        let core = "fn f() { let _ = h::<core::convert::Infallible, _>(x); }";
        assert!(run_on(async_graphql).is_empty());
        assert!(run_on(bare).is_empty());
        assert!(run_on(core).is_empty());
    }

    #[test]
    fn allows_let_underscore_in_never_returning_fn() {
        // Regression for #6555: a `-> !` function is compiler-guaranteed to
        // diverge, so a discarded fallible result can never propagate to a
        // caller. The issue's exact fd `exit_codes.rs` shape — `process::exit`
        // sits at the fn level while the `let _ =` is nested inside `unsafe`/`if`,
        // which the block-level exit scan misses.
        let fd = r#"
            pub fn exit(self) -> ! {
                if self == ExitCode::KilledBySigint {
                    unsafe {
                        if signal(Signal::SIGINT, SigHandler::SigDfl).is_ok() {
                            let _ = raise(Signal::SIGINT);
                        }
                    }
                }
                process::exit(self.into())
            }
        "#;
        // Minimal divergent body that does not itself contain `process::exit`.
        let looping = r#"
            fn run() -> ! {
                let _ = fallible();
                loop {}
            }
        "#;
        assert!(run_on(fd).is_empty());
        assert!(run_on(looping).is_empty());
    }

    #[test]
    fn flags_let_underscore_in_non_never_returning_fn() {
        // Negative space for #6555: the exemption requires a bare `-> !` return
        // type. A fn returning `()`, a `Result<…>`, or a `Result<!, E>` (where
        // `!` is buried in a generic, not the top-level return type) can still
        // propagate errors, so a discarded fallible call must still fire.
        let unit = "fn f() -> () { let _ = fallible(); }";
        let result = "fn f() -> Result<(), E> { let _ = fallible(); Ok(()) }";
        let result_never_err = "fn f() -> Result<!, E> { let _ = fallible(); loop {} }";
        let no_return_type = "fn f() { let _ = fallible(); }";
        assert_eq!(run_on(unit).len(), 1);
        assert_eq!(run_on(result).len(), 1);
        assert_eq!(run_on(result_never_err).len(), 1);
        assert_eq!(run_on(no_return_type).len(), 1);
    }

    #[test]
    fn flags_let_underscore_non_infallible_turbofish() {
        // Negative space for #3962: the exemption is keyed on an `Infallible`
        // final segment. A turbofish with a real error type still genuinely
        // swallows the error, and a fallible call with no turbofish at all is
        // unaffected by this exemption.
        let real_error = "fn f() { let _ = f::<MyError, _>(x); }";
        let no_turbofish = "fn f() { let _ = fallible(); }";
        assert_eq!(run_on(real_error).len(), 1);
        assert_eq!(run_on(no_turbofish).len(), 1);
    }

    #[test]
    fn allows_let_underscore_output_param_binding_macro() {
        // Regression for #7009: `syn`'s parsing macros use output-parameter
        // binding (`var in stream`) — the parse result is written into the
        // pre-declared `content`, and the discarded return value is only the
        // delimiter-span metadata, not an error. The issue's exact tracing
        // `tracing-attributes/src/attr.rs` examples.
        let parenthesized =
            "fn f(input: ParseStream) { let content; let _ = syn::parenthesized!(content in input); }";
        let braced = "fn f() { let _ = syn::braced!(content in input); }";
        // Unqualified, different macro name — proves the exemption keys on the
        // `<ident> in <ident>` token sequence, not a macro-name allowlist.
        let bracketed = "fn f() { let _ = bracketed!(inner in stream); }";
        assert!(run_on(parenthesized).is_empty());
        assert!(run_on(braced).is_empty());
        assert!(run_on(bracketed).is_empty());
    }

    #[test]
    fn flags_let_underscore_macro_without_in_binding() {
        // Negative space for #7009: the exemption requires the `<ident> in
        // <ident>` binding sequence. A macro invocation with ordinary
        // comma-separated arguments carries no output-parameter binding, so a
        // discarded result there genuinely swallows a potential error and fires.
        let comma_args = "fn f() { let _ = some_macro!(a, b); }";
        let single_arg = "fn f() { let _ = try_parse!(input); }";
        assert_eq!(run_on(comma_args).len(), 1);
        assert_eq!(run_on(single_arg).len(), 1);
    }
}
