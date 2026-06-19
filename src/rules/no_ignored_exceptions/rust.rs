//! no-ignored-exceptions Rust backend — flag `let _ = fallible()` that
//! discards a Result/Option without handling it.
//!
//! Tests are exempted: a `let _ = fn_under_test()` pattern is the
//! idiomatic way to assert "this call doesn't panic" without caring
//! about the return value. Exempt when in a test context — a
//! `#[test]`/`#[cfg(test)]` attribute walk via
//! `rust_helpers::is_in_test_context` — or when the file lives under a
//! `tests/` directory (`rust_helpers::is_under_tests_dir`), since plain
//! helper fns there are integration-test infrastructure.
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
//!   it. Scoped to receiver chains rooted at `stderr()`/`stdout()` so a plain
//!   `let _ = file.write_all(..)` still fires. The macro spellings of the same
//!   idiom are also exempt: `print!`/`println!`/`eprint!`/`eprintln!` target a
//!   standard stream by definition, and `write!`/`writeln!` only when their
//!   first argument (the writer) roots at `stderr()`/`stdout()` — so a plain
//!   `let _ = writeln!(file, ..)` to an ordinary writer still fires.
//! - `let _ = expr.write_str(..)` / `let _ = expr.write_char(..)`: the
//!   `core::fmt::Write` trait methods. Writing to an in-memory buffer (`String`,
//!   `fmt::Formatter`, a `Vec<u8>` wrapper) yields a `fmt::Result` that is
//!   structurally `Ok(())`, so discarding it with `let _ =` drops an always-Ok
//!   unit, not an error. The method names `write_str`/`write_char` are specific
//!   to `fmt::Write` (distinct from `io::Write`'s `write`/`write_all`), so this
//!   is exempt by method name without type inference — a plain
//!   `let _ = file.write_all(..)` still fires.
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
//! - `let _ = fallible()` inside the `fn drop` of an `impl Drop for ...` block:
//!   `Drop::drop` returns `()`, so an error has no way to be propagated to the
//!   caller (no `?`, no `Result` return). `let _ =` is the idiomatic best-effort
//!   cleanup for an RAII destructor. Scoped to a `fn drop` directly inside a
//!   `Drop` trait impl, so a `let _ =` in any other method still fires.
//! - `let _ = f::<Infallible, _>(..)`: a turbofish call whose type arguments fix
//!   the error type to `Infallible`. `Result<_, Infallible>` is uninhabited on
//!   its `Err` side, so the result can never be `Err` — discarding it ignores no
//!   error. Recognized purely syntactically on the turbofish: any type argument
//!   whose final path segment is `Infallible` (`Infallible`,
//!   `std::convert::Infallible`, `core::convert::Infallible`). A turbofish with a
//!   real error type (`let _ = f::<MyError, _>(..)`) or no turbofish at all still
//!   fires.
//!
//! NOTE: This rule uses a heuristic (call-like pattern matching) rather than
//! type awareness. It may flag `let _ = infallible_fn()` where the function
//! provably does not return Result/Option. Without --type-aware, there is no
//! fix for this class of FP — document intent in the calling code if needed.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{is_in_test_context, is_under_tests_dir};
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

    // Skip inside tests — `let _ = …` there is "call and don't care". Covers
    // both a `#[test]`/`#[cfg(test)]` attribute context and a file under a
    // `tests/` directory, where plain helper fns are test infrastructure too.
    if is_in_test_context(node, source) || is_under_tests_dir(ctx.path) {
        return;
    }

    // Skip compile-fail test fixtures (`tests/.../fail/`): `let _ =` there
    // suppresses "unused result" warnings in the expected compiler output.
    if is_compile_fail_fixture(ctx.path) {
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

    // Skip the best-effort channel fire-and-forget idiom `let _ = expr.send(..)`:
    // an `Err` only signals the receiver dropped on a shutdown/cleanup path.
    if is_channel_send(value, source) {
        return;
    }

    // Skip the best-effort standard-stream write `let _ = stderr()/stdout()...`:
    // the I/O error has nowhere to go in an error-reporting path.
    if is_std_stream_write(value, source) {
        return;
    }

    // Skip the `core::fmt::Write` idiom `let _ = expr.write_str/write_char(..)`:
    // on an in-memory buffer the `fmt::Result` is always `Ok(())`.
    if is_fmt_write_method(value, source) {
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

    // Skip best-effort cleanup in a `Drop::drop` body: `drop` returns `()`, so
    // the error has no way to be propagated — `let _ =` is the idiomatic form.
    if is_in_drop_impl(node, source) {
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

/// True if `node` sits inside the `fn drop` of an `impl Drop for ...` block.
///
/// Walks ancestors to the nearest enclosing `function_item`; it must be named
/// `drop`, and its nearest enclosing `impl_item` must have a `trait` field whose
/// final path segment is `Drop` (so `impl std::ops::Drop for T` also matches).
/// Both conditions are required: a method named `drop` in an inherent impl, or
/// any other method in a `Drop` impl, does not qualify. In `Drop::drop` the
/// return type is `()`, so a fallible result discarded with `let _ =` is the
/// idiomatic best-effort cleanup — there is no channel to propagate the error.
fn is_in_drop_impl(node: Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "function_item" {
            let Some(name) = ancestor.child_by_field_name("name") else {
                return false;
            };
            if name.utf8_text(source) != Ok("drop") {
                return false;
            }
            return enclosing_impl_is_drop(ancestor, source);
        }
        current = ancestor.parent();
    }
    false
}

/// True if the nearest `impl_item` enclosing `func` is a `Drop` trait impl.
fn enclosing_impl_is_drop(func: Node, source: &[u8]) -> bool {
    let mut current = func.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "impl_item" {
            let Some(trait_node) = ancestor.child_by_field_name("trait") else {
                return false;
            };
            let Ok(text) = trait_node.utf8_text(source) else {
                return false;
            };
            return text.rsplit("::").next().unwrap_or(text).trim() == "Drop";
        }
        current = ancestor.parent();
    }
    false
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
/// argument (the writer) heads at a `stderr()`/`stdout()` call.
fn is_std_stream_write_macro(value: Node, source: &[u8]) -> bool {
    let Some(macro_name_node) = value.child_by_field_name("macro") else {
        return false;
    };
    let Ok(macro_name) = macro_name_node.utf8_text(source) else {
        return false;
    };
    // Last segment for a qualified `std::writeln!` style invocation.
    let name = macro_name.rsplit("::").next().unwrap_or(macro_name);
    match name {
        "print" | "println" | "eprint" | "eprintln" => true,
        "write" | "writeln" => {
            // The token tree is an unnamed child (no `token_tree` field exists).
            let mut cursor = value.walk();
            let Some(token_tree) = value
                .children(&mut cursor)
                .find(|child| child.kind() == "token_tree")
            else {
                return false;
            };
            macro_first_arg_heads_at_std_stream(token_tree, source)
        }
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
/// walking through any intermediate method calls (e.g. `stderr().lock()`).
fn receiver_roots_at_std_stream(mut node: Node, source: &[u8]) -> bool {
    loop {
        match node.kind() {
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
        // Negative space for #3997: `write!`/`writeln!` are exempt ONLY when
        // their first argument roots at a std stream. A write to an ordinary
        // writer (file, buffer) genuinely swallows the error and must fire. A
        // non-write macro (`some_macro!`, `vec!`) is likewise not exempt.
        let writeln_file = r#"fn f(mut file: File) { let _ = writeln!(file, "{x}"); }"#;
        let write_buf = r#"fn f(buf: &mut String) { let _ = write!(buf, "x"); }"#;
        let other_macro = "fn f() { let _ = some_macro!(x); }";
        assert_eq!(run_on(writeln_file).len(), 1);
        assert_eq!(run_on(write_buf).len(), 1);
        assert_eq!(run_on(other_macro).len(), 1);
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
    fn flags_let_underscore_in_non_test_path() {
        // Negative space for #3298: the `tests/` exemption is scoped to test
        // infra. The same discarded fallible call in production code (not under
        // `tests/`, no `#[test]`) genuinely swallows the error and must fire.
        let src = "pub fn cleanup() { let _ = fallible(); }";
        let diagnostics = crate::rules::test_helpers::run_rule(&Check, src, "src/lib.rs");
        assert_eq!(diagnostics.len(), 1);
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
}
