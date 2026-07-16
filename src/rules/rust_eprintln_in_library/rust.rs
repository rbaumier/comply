//! rust-eprintln-in-library backend.
//!
//! Walks `macro_invocation` nodes for `eprintln!` / `eprint!` and
//! flags any invocation that:
//!
//! - is a bare `eprintln!`/`eprint!` (which the prelude may resolve to
//!   std's) or an explicit `std::`/`core::` qualification — a path-qualified
//!   macro from any other crate (`anstream::eprintln!`) is a different,
//!   redirectable macro and is exempt, and
//! - is **not** in test context (`#[test]` / `#[cfg(test)]` /
//!   `tests/` integration directory / an inline-test-module file named
//!   `tests.rs` or `test.rs`), and
//! - is **not** in a Cargo build script (`build.rs`), and
//! - is **not** in a binary file (`main.rs`, `src/bin/*.rs`), and
//! - is **not** in a file declared as an explicit-path executable target
//!   in the nearest `Cargo.toml` (a `[[bin]]`/`[[example]]`/`[[bench]]`/
//!   `[[test]]` table with `path = "utils/foo.rs"`), and
//! - is **not** in a crate that declares a binary (the nearest
//!   `Cargo.toml` declares a `[[bin]]` target or a `src/main.rs`
//!   exists next to it), and
//! - is **not** in a build-time codegen crate (the nearest `Cargo.toml`
//!   `[package].name` ends with `-build`/`-codegen`/`-bindgen` or the
//!   `_` variants), and
//! - is **not** in an FFI bridge crate (the nearest `Cargo.toml`
//!   `[lib] crate-type` declares `cdylib`/`staticlib` and no `rlib`/`lib`), and
//! - is **not** in the `then` branch of an `if` gated by a
//!   verbose/debug-style flag (`if self.verbose() { eprintln!(...) }`).
//!
//! `eprintln!` is fine in CLI binaries — that's where it belongs.
//! It's a problem in libraries because consumers can't redirect or
//! capture it. A crate that ships a binary is an application: every
//! one of its source files is exempt, not just the entry points —
//! even when it also carries a `[lib]` purely to expose internals to
//! its own integration tests (the `lib.rs` + `main.rs` split).
//!
//! A file declared as an explicit-path executable target — a `[[bin]]`,
//! `[[example]]`, `[[bench]]`, or `[[test]]` table with `path = "utils/foo.rs"`
//! — is itself a standalone binary with its own `fn main()` that Cargo compiles
//! and runs directly. It is application code even when it lives outside the
//! conventional `src/main.rs` / `src/bin/` locations, so its `eprintln!` is
//! exempt regardless of whether the surrounding crate also ships a library.
//!
//! A Cargo build script (`build.rs`) is a separate binary that Cargo
//! compiles and runs at build time, not the crate's runtime library code.
//! Cargo captures and displays its stderr, so `eprintln!` is the idiomatic
//! build-script diagnostic channel — it is exempt.
//!
//! A build-time codegen crate (a `-build`/`-codegen`/`-bindgen` library
//! such as `prost-build` or `tonic-build`) is consumed from a `build.rs`
//! script, where writing to Cargo's build-output stream via `eprintln!` /
//! `println!` is the idiomatic diagnostic channel — tracing/log is
//! unavailable there — so its `eprintln!` is exempt too.
//!
//! A `proc-macro = true` crate runs at compile time during macro expansion:
//! its `eprintln!` writes to the compiler's build-time stderr, not the final
//! program's runtime stderr, and the macro is not callable at runtime at all.
//! The rule's premise — "library consumers can't redirect or capture it at
//! runtime" — does not hold, so `eprintln!` in a proc-macro crate is exempt.
//!
//! An FFI bridge crate (a `[lib] crate-type` of `cdylib`/`staticlib` with no
//! `rlib`/`lib`, such as Python/Java/Swift bindings) is linked into a foreign
//! runtime, not consumed as a Rust library. That runtime never initialises a
//! Rust tracing subscriber, so there is no `tracing::warn!` alternative —
//! `eprintln!` is the only practical way to surface errors at the FFI boundary,
//! so it is exempt too.
//!
//! A logging/tracing infrastructure crate (the nearest `Cargo.toml`
//! `[package].name` is a known logging crate such as `tracing` /
//! `tracing-subscriber` / `env_logger`, or carries a `tracing` / `logger` /
//! `logging` / `slog` segment) implements the
//! `Subscriber` / `Log` machinery itself. It cannot route its own internal
//! failures through `tracing::warn!` / `log::error!` — that is the very system
//! that has failed (a log-file rotation error, a formatter bug, a
//! `RUST_LOG` parse error) or would recurse — so `eprintln!` is its
//! legitimate last-resort fallback output and is exempt. The match is on the
//! crate's own identity, not on whether it depends on a logging crate, so an
//! application that merely uses `tracing` stays flagged.
//!
//! A custom panic hook installed via `std::panic::set_hook(|info| { … })`
//! exists to write a human-readable crash report to stderr before the
//! process dies — that is exactly what the default hook does. At the point
//! the closure runs the program is unwinding from a panic, so a `tracing`
//! subscriber may already be torn down and a panic hook is expected to be
//! minimal and dependency-free; `eprintln!` / `eprint!` is the correct and
//! idiomatic output channel there. An `eprintln!` whose enclosing closure is
//! the hook argument to a `set_hook` call (directly, or wrapped in
//! `Box::new(…)`) is exempt. Output in a *different* closure nested inside
//! the hook body stays flagged.
//!
//! An `eprintln!` / `eprint!` whose immediately-following statement in the
//! enclosing block unconditionally terminates the process is a
//! pre-termination diagnostic: the process dies on that very next statement,
//! so the rule's premise — consumers can't redirect or capture the output —
//! is moot, there is nothing left to redirect. This is the same category as
//! the panic-hook exemption (output written just before the process dies).
//! The terminator is either an `unreachable!()` / `panic!(…)` invocation
//! (matched on the macro's final path segment) or a `std::process::exit(…)` /
//! `std::process::abort()` call (the callee's final segment is `exit`/`abort`
//! qualified by a `process` segment). The next statement must itself be the
//! terminator; an `eprintln!` followed by ordinary code, or one that is the
//! last statement of its block with no terminator after it, stays flagged.
//!
//! Output gated behind a runtime verbosity flag is opt-in diagnostics,
//! not unconditional library noise: the consumer only sees it after
//! turning the flag on. The guard is recognised when the `if` condition
//! is either:
//!
//! - a *simple* flag reference — a bare identifier, a field access, or a
//!   no-argument method call — whose final segment names a known flag
//!   (`verbose`, `debug`, `quiet`, `trace`, …), or
//! - an environment-variable-presence check — `env::var(KEY).is_ok()` or
//!   `env::var_os(KEY).is_some()` (with or without a `std::` prefix), or
//! - an environment-variable value-equality check —
//!   `env::var(KEY).as_deref() == Ok("…")` or the `!=` form (with or
//!   without a `std::` prefix): the variable's value is compared against a
//!   specific string literal, a strictly stronger opt-in than presence.
//!
//! Both env-var forms make the `eprintln!` a runtime opt-in — it only runs
//! once the consumer sets the variable — just like a verbosity flag. A
//! negated verbosity flag, or an env-var check in any other shape, stays
//! flagged.
//!
//! The same opt-in covers the inverted early-return form
//! `if !self.debug { return; }` written at a function's entry: when the
//! first statement of a function body is an `if` whose condition is a
//! negated *simple* flag reference (`!verbose`, `!self.debug`) and whose
//! body is a single `return`, the function bails out unless the flag is on,
//! so every `eprintln!` in the rest of that function only runs when the flag
//! is set — exactly the positive `if self.debug { … }` case, control-flow
//! inverted. Only the first statement counts as the entry guard.
//!
//! An `eprintln!` / `eprint!` covered by an explicit
//! `#[allow(clippy::disallowed_macros)]` / `#[expect(clippy::disallowed_macros)]`
//! is exempt. `eprintln!` is a canonical entry in clippy's `disallowed_macros`
//! lint, so that attribute is the author explicitly opting out of exactly this
//! macro ban — scoped to the enclosing item or statement (e.g. an
//! `#[allow(clippy::disallowed_macros)] { eprintln!(…) }` block). The same
//! attribute-honoring applies as for any clippy-mirroring rule. A comment alone
//! (with no such attribute) does not exempt — only the attribute does.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{
    is_in_test_context, is_suppressed_by_clippy_allow, is_under_tests_dir,
};
use std::path::Path;

const KINDS: &[&str] = &["macro_invocation"];

/// Final-segment names that mark an `if` condition as a runtime
/// verbose/debug flag. Kept deliberately small: only output gated by a
/// recognised diagnostics flag is opt-in, everything else stays flagged.
const VERBOSE_FLAG_NAMES: &[&str] = &[
    "verbose",
    "debug",
    "quiet",
    "trace",
    "is_verbose",
    "is_debug",
    "is_quiet",
    "is_trace",
    "debug_mode",
    "verbose_mode",
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
        let source_bytes = ctx.source.as_bytes();
        let Some(macro_name) = node.child_by_field_name("macro") else {
            return;
        };
        let name = macro_name.utf8_text(source_bytes).unwrap_or("");
        let bare = name.rsplit("::").next().unwrap_or(name);
        if bare != "eprintln" && bare != "eprint" {
            return;
        }
        // A path-qualified macro from a third-party crate (e.g. `anstream::eprintln!`)
        // is a different macro with its own (redirectable) output semantics — not the
        // std macro this rule targets. Only a bare invocation (which the prelude may
        // resolve to std's) or an explicit `std::`/`core::` qualification fires. A
        // leading global-path `::` (`::std::eprintln!`) is normalized away first so the
        // std/core forms still fire regardless of the global-path prefix.
        let qualified = name.trim_start_matches("::");
        if qualified.contains("::")
            && !qualified.starts_with("std::")
            && !qualified.starts_with("core::")
        {
            return;
        }
        if is_in_test_context(node, source_bytes) || is_under_tests_dir(ctx.path) {
            return;
        }
        if ctx.project.in_mdbook_project(ctx.path) {
            return;
        }
        if crate::rules::path_utils::is_rust_build_script(ctx.path) {
            return;
        }
        if is_binary_file(ctx.path) {
            return;
        }
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.declares_binary() || m.declares_executable_at(ctx.path))
        {
            return;
        }
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.is_build_codegen_crate())
        {
            return;
        }
        // A `proc-macro = true` crate runs at compile time during macro
        // expansion: its `eprintln!` goes to the compiler's build-time stderr,
        // not the shipped program's runtime stderr, and the macro is not
        // callable at runtime. The "consumers can't capture it at runtime"
        // premise is structurally inapplicable, so it is exempt.
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.is_proc_macro())
        {
            return;
        }
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.is_ffi_bridge_crate())
        {
            return;
        }
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.is_logging_infra_crate())
        {
            return;
        }
        if is_under_verbose_flag_guard(node, source_bytes)
            || is_after_inverted_early_return_guard(node, source_bytes)
        {
            return;
        }
        if is_in_panic_hook_closure(node, source_bytes) {
            return;
        }
        if is_pre_termination_diagnostic(node, source_bytes) {
            return;
        }
        if is_suppressed_by_clippy_allow(node, &["disallowed_macros"], source_bytes) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            std::sync::Arc::clone(&ctx.path_arc),
            &node,
            "rust-eprintln-in-library",
            format!(
                "`{bare}!` writes to stderr directly — library consumers \
                 can't redirect, configure, or capture it. Use \
                 `tracing::warn!` / `tracing::error!` instead."
            ),
            Severity::Warning,
        ));
    }
}

/// True if `path` is a binary entry point: `main.rs` at any directory
/// level, or any file under a `bin/` directory.
fn is_binary_file(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str())
        && name == "main.rs"
    {
        return true;
    }
    path.components().any(|c| c.as_os_str() == "bin")
}

/// True when `node` sits in the `then` branch of an enclosing `if`
/// whose condition is a simple verbose/debug-flag reference. Walks
/// every ancestor `if_expression` (so a flag guard several blocks up
/// still exempts), requiring the node to be inside the `consequence`
/// (not the `else`) of at least one of them.
fn is_under_verbose_flag_guard(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "if_expression"
            && let Some(consequence) = parent.child_by_field_name("consequence")
            && is_descendant_of(node, consequence)
            && parent
                .child_by_field_name("condition")
                .is_some_and(|cond| is_verbose_flag_condition(cond, source))
        {
            return true;
        }
        current = parent;
    }
    false
}

/// True when `node` sits in a function whose body opens with an inverted
/// early-return verbose guard — `fn f() { if !self.debug { return; } … }`.
/// Such a guard bails the function out unless the flag is on, so every
/// statement after it (including this `eprintln!`) runs only in the opt-in
/// case: the control-flow-inverted twin of `if self.debug { eprintln!(…) }`.
/// Conservative: only the *first* statement of the nearest enclosing
/// function body is treated as the entry guard.
fn is_after_inverted_early_return_guard(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = enclosing_function_body(node) else {
        return false;
    };
    let Some(first) = first_statement(body) else {
        return false;
    };
    is_inverted_verbose_return_guard(first, source)
}

/// The `body` block of the nearest enclosing `function_item`, or `None` if
/// `node` is not inside one.
fn enclosing_function_body(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "function_item" {
            return parent.child_by_field_name("body");
        }
        current = parent;
    }
    None
}

/// The first non-comment statement of a `block`, or `None` if empty.
fn first_statement(block: tree_sitter::Node) -> Option<tree_sitter::Node> {
    (0..block.named_child_count())
        .filter_map(|i| block.named_child(i))
        .find(|n| n.kind() != "line_comment" && n.kind() != "block_comment")
}

/// True when `stmt` is an inverted verbose-flag early-return guard:
/// `if !<verbose-flag> { return; }`. The condition must be a `!`-negated
/// simple flag reference whose final segment is in `VERBOSE_FLAG_NAMES`
/// (reusing the positive form's `flag_segment` extraction), and the
/// consequence must be a block whose only statement is a `return`.
fn is_inverted_verbose_return_guard(stmt: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(if_expr) = unwrap_if_expression(stmt) else {
        return false;
    };
    let condition_negates_flag = if_expr
        .child_by_field_name("condition")
        .is_some_and(|cond| is_negated_verbose_flag(cond, source));
    if !condition_negates_flag {
        return false;
    }
    if_expr
        .child_by_field_name("consequence")
        .is_some_and(is_single_return_block)
}

/// Unwrap a statement to its `if_expression`: an `if` used as a statement is
/// wrapped in an `expression_statement`, but may also appear bare.
fn unwrap_if_expression(stmt: tree_sitter::Node) -> Option<tree_sitter::Node> {
    match stmt.kind() {
        "if_expression" => Some(stmt),
        "expression_statement" => stmt
            .named_child(0)
            .filter(|inner| inner.kind() == "if_expression"),
        _ => None,
    }
}

/// True when `cond` is `!<flag>` — a `unary_expression` with the `!` operator
/// applied to a simple flag reference whose final segment is a known
/// verbose/debug flag name. Reuses `flag_segment` so the negated operand
/// (`!self.debug`, `!verbose`, `!self.verbose()`) resolves its flag name
/// identically to the positive `if self.debug { … }` form.
fn is_negated_verbose_flag(cond: tree_sitter::Node, source: &[u8]) -> bool {
    if cond.kind() != "unary_expression" {
        return false;
    }
    let is_not = cond
        .child(0)
        .and_then(|op| op.utf8_text(source).ok())
        .is_some_and(|op| op == "!");
    if !is_not {
        return false;
    }
    cond.named_child(0)
        .and_then(|operand| flag_segment(operand, source))
        .is_some_and(|seg| VERBOSE_FLAG_NAMES.contains(&seg))
}

/// True when `block` is a `{ … }` whose single non-comment statement is a
/// `return` (with or without a value, `;`-terminated or trailing).
fn is_single_return_block(block: tree_sitter::Node) -> bool {
    if block.kind() != "block" {
        return false;
    }
    let mut stmts = (0..block.named_child_count())
        .filter_map(|i| block.named_child(i))
        .filter(|n| n.kind() != "line_comment" && n.kind() != "block_comment");
    let Some(only) = stmts.next() else {
        return false;
    };
    if stmts.next().is_some() {
        return false;
    }
    is_return_statement(only)
}

/// True when `node` is a `return` — a bare/trailing `return_expression` or an
/// `expression_statement` wrapping one (`return;`).
fn is_return_statement(node: tree_sitter::Node) -> bool {
    match node.kind() {
        "return_expression" => true,
        "expression_statement" => node
            .named_child(0)
            .is_some_and(|inner| inner.kind() == "return_expression"),
        _ => false,
    }
}

/// True if `node` is `ancestor` or nested anywhere inside it.
fn is_descendant_of(node: tree_sitter::Node, ancestor: tree_sitter::Node) -> bool {
    let mut current = Some(node);
    while let Some(n) = current {
        if n == ancestor {
            return true;
        }
        current = n.parent();
    }
    false
}

/// True when `node` sits inside the closure passed to a panic-hook
/// installer — `std::panic::set_hook(…)` / `panic::set_hook(…)` /
/// `set_hook(…)`. Finds the nearest enclosing `closure_expression`, then
/// checks that closure is the hook argument of a `set_hook` call, allowing
/// a single `Box::new(…)` wrapper between the closure and the call (the
/// canonical `set_hook(Box::new(|info| …))` shape). A panic hook writes the
/// crash report to stderr by design, so its `eprintln!` / `eprint!` is
/// intended output, not stray library noise.
///
/// The closure must itself be the hook argument: an `eprintln!` in a
/// *different* closure nested deeper in the hook body resolves to that
/// inner closure, which is not a `set_hook` argument, so it stays flagged.
fn is_in_panic_hook_closure(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "closure_expression" {
            return closure_is_set_hook_argument(parent, source);
        }
        current = parent;
    }
    false
}

/// True when `closure` is the argument to a `set_hook` call, possibly via a
/// single wrapping call (in practice `Box::new(closure)`, as `set_hook` takes
/// a `Box<dyn Fn>`). The closure is the hook argument when it is a direct
/// argument to a `set_hook` call, or when the one call it is a direct argument
/// to is itself a direct argument to a `set_hook` call
/// (`set_hook(Box::new(|info| …))`). Only the outer call must be `set_hook`;
/// the single wrapper's callee is not constrained.
fn closure_is_set_hook_argument(closure: tree_sitter::Node, source: &[u8]) -> bool {
    if is_set_hook_argument(closure, source) {
        return true;
    }
    // `set_hook(Box::new(closure))`: the closure's enclosing call (the
    // `Box::new(…)`) is itself the argument to `set_hook`.
    enclosing_call(closure).is_some_and(|call| is_set_hook_argument(call, source))
}

/// True when `node` is a direct argument of a `call_expression` whose callee
/// path ends in `set_hook`.
fn is_set_hook_argument(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(args) = node.parent() else {
        return false;
    };
    if args.kind() != "arguments" {
        return false;
    }
    let Some(call) = args.parent() else {
        return false;
    };
    if call.kind() != "call_expression" {
        return false;
    }
    call.child_by_field_name("function")
        .is_some_and(|f| callee_ends_in_set_hook(f, source))
}

/// If `node` is a direct argument of a `call_expression`, return that call.
fn enclosing_call(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let args = node.parent()?;
    if args.kind() != "arguments" {
        return None;
    }
    let call = args.parent()?;
    (call.kind() == "call_expression").then_some(call)
}

/// True when a call's callee names `set_hook` as its final path segment —
/// `std::panic::set_hook`, `panic::set_hook`, or a bare `set_hook`.
fn callee_ends_in_set_hook(func: tree_sitter::Node, source: &[u8]) -> bool {
    match func.kind() {
        "identifier" => func.utf8_text(source).ok() == Some("set_hook"),
        "scoped_identifier" => {
            func.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                == Some("set_hook")
        }
        _ => false,
    }
}

/// True when `node` (an `eprintln!` / `eprint!` `macro_invocation`) is the
/// direct predecessor of an unconditional process termination in its enclosing
/// block: the statement immediately following it is an `unreachable!()` /
/// `panic!(…)` invocation, or a `std::process::exit(…)` / `std::process::abort()`
/// call. Such an `eprintln!` is a pre-termination diagnostic — the process
/// terminates on the next statement, so there is nothing for a consumer to
/// redirect or capture, the same rationale as the panic-hook exemption.
///
/// Resolves the statement that wraps the `eprintln!` (its ancestor that is a
/// direct child of a `block`), then its next sibling statement, skipping
/// comments. An `eprintln!` that is its block's last statement has no
/// following sibling and is not exempted, so it still fires.
fn is_pre_termination_diagnostic(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(stmt) = statement_in_block(node) else {
        return false;
    };
    let Some(next) = next_statement(stmt) else {
        return false;
    };
    let terminates_via_macro = macro_invocation_of(next)
        .and_then(|mi| macro_invocation_name(mi, source))
        .is_some_and(|name| name == "unreachable" || name == "panic");
    terminates_via_macro || is_process_exit_call(next, source)
}

/// True when `stmt` is a `std::process::exit(…)` / `std::process::abort()`
/// call — an unconditional process termination. The callee must be a
/// `scoped_identifier` whose final segment is `exit`/`abort` and whose
/// preceding segment is `process` (matching `std::process::exit`,
/// `process::exit`, `std::process::abort`, …). Requiring the `process`
/// qualifier keeps this a structural match on the standard-library terminators
/// rather than a bare-name match over any local `exit`/`abort` function.
/// Reuses `trailing_path_segment` to read the qualifier, mirroring
/// `is_env_var_call`'s `env::var` check.
fn is_process_exit_call(stmt: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(call) = call_expression_of(stmt) else {
        return false;
    };
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "scoped_identifier" {
        return false;
    }
    let Some(name) = func
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
    else {
        return false;
    };
    if name != "exit" && name != "abort" {
        return false;
    }
    func.child_by_field_name("path")
        .is_some_and(|path| trailing_path_segment(path, source) == Some("process"))
}

/// The `call_expression` a statement node carries: the node itself when it is a
/// trailing-expression `call_expression` (`process::exit(1)` with no `;`), or
/// the sole inner call of an `expression_statement` (`process::exit(1);`). Any
/// other statement shape yields `None`. Mirrors `macro_invocation_of`.
fn call_expression_of(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    match node.kind() {
        "call_expression" => Some(node),
        "expression_statement" => node
            .named_child(0)
            .filter(|inner| inner.kind() == "call_expression"),
        _ => None,
    }
}

/// The ancestor of `node` that is a direct child of a `block` — the statement
/// (or trailing expression) `node` belongs to within its nearest enclosing
/// block — or `None` when `node` is not inside a block.
fn statement_in_block(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "block" {
            return Some(current);
        }
        current = parent;
    }
    None
}

/// The next non-comment named sibling of `stmt`, or `None` when `stmt` is the
/// last named child. A `line_comment` / `block_comment` between two statements
/// is skipped so the genuinely-following statement is examined.
fn next_statement(stmt: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut sibling = stmt.next_named_sibling();
    while let Some(n) = sibling {
        if n.kind() != "line_comment" && n.kind() != "block_comment" {
            return Some(n);
        }
        sibling = n.next_named_sibling();
    }
    None
}

/// The `macro_invocation` a statement node carries: the node itself when it is
/// a trailing-expression `macro_invocation` (`unreachable!()` with no `;`), or
/// the sole inner macro of an `expression_statement` (`unreachable!();`). Any
/// other statement shape yields `None`.
fn macro_invocation_of(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    match node.kind() {
        "macro_invocation" => Some(node),
        "expression_statement" => node
            .named_child(0)
            .filter(|inner| inner.kind() == "macro_invocation"),
        _ => None,
    }
}

/// The final path segment of a `macro_invocation`'s name (`std::unreachable` →
/// `unreachable`), or `None` when the `macro` field is absent.
fn macro_invocation_name<'a>(mi: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let name = mi.child_by_field_name("macro")?.utf8_text(source).ok()?;
    Some(name.rsplit("::").next().unwrap_or(name))
}

/// True when `cond` is a recognised runtime opt-in guard: either a
/// *simple* flag reference (a bare identifier, a field access, or a
/// no-argument method call) whose final path segment is a known
/// verbose/debug flag name, or an environment-variable opt-in check
/// (`env::var(KEY).is_ok()` / `env::var_os(KEY).is_some()`, or
/// `env::var(KEY).as_deref() ==/!= Ok("…")`). A negated flag
/// (`!self.verbose()`) or any other compound expression returns false —
/// those are not plain "is the flag on" guards and stay flagged.
fn is_verbose_flag_condition(cond: tree_sitter::Node, source: &[u8]) -> bool {
    flag_segment(cond, source).is_some_and(|seg| VERBOSE_FLAG_NAMES.contains(&seg))
        || is_env_var_presence_condition(cond, source)
}

/// True when `cond` is an environment-variable opt-in check, in either shape:
///
/// - presence — `env::var(KEY).is_ok()` / `env::var_os(KEY).is_some()`: a
///   `.is_ok()` / `.is_some()` method call whose receiver is a call to
///   `env::var` / `env::var_os`, or
/// - value-equality — `env::var(KEY).as_deref() == Ok("…")` (or the `!=`
///   form): an `env::var(...).as_deref()` operand compared against an
///   `Ok(<string_literal>)`.
///
/// Both are `std::`-prefix optional. The value-equality form is a strictly
/// stronger opt-in — the `eprintln!` fires only when the consumer sets the
/// variable to that specific value — so it is a runtime opt-in like the
/// presence form and a verbosity flag.
fn is_env_var_presence_condition(cond: tree_sitter::Node, source: &[u8]) -> bool {
    // `env::var(KEY).as_deref() ==/!= Ok("value")`
    if cond.kind() == "binary_expression" {
        return is_env_var_value_equality(cond, source);
    }
    // `<receiver>.is_ok()` / `<receiver>.is_some()`
    if cond.kind() != "call_expression" {
        return false;
    }
    let Some(func) = cond.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "field_expression" {
        return false;
    }
    let presence_ok = func
        .child_by_field_name("field")
        .and_then(|f| f.utf8_text(source).ok())
        .is_some_and(|m| m == "is_ok" || m == "is_some");
    if !presence_ok {
        return false;
    }
    // The receiver must be a call to `env::var` / `env::var_os`.
    func.child_by_field_name("value")
        .is_some_and(|recv| is_env_var_call(recv, source))
}

/// True when `cond` is `env::var(KEY).as_deref() == Ok("…")` or the `!=`
/// form: a `==`/`!=` `binary_expression` with one operand an
/// `env::var(...).as_deref()` call and the other an `Ok(<string_literal>)`.
/// The order of the two operands is not constrained.
fn is_env_var_value_equality(cond: tree_sitter::Node, source: &[u8]) -> bool {
    let is_eq_or_ne = cond
        .child_by_field_name("operator")
        .and_then(|op| op.utf8_text(source).ok())
        .is_some_and(|op| op == "==" || op == "!=");
    if !is_eq_or_ne {
        return false;
    }
    let (Some(left), Some(right)) = (
        cond.child_by_field_name("left"),
        cond.child_by_field_name("right"),
    ) else {
        return false;
    };
    (is_env_var_as_deref_call(left, source) && is_ok_string_literal(right, source))
        || (is_env_var_as_deref_call(right, source) && is_ok_string_literal(left, source))
}

/// True when `node` is `env::var(KEY).as_deref()` — an `.as_deref()` method
/// call whose receiver is a call to `env::var` / `env::var_os`.
fn is_env_var_as_deref_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "field_expression" {
        return false;
    }
    let is_as_deref = func
        .child_by_field_name("field")
        .and_then(|f| f.utf8_text(source).ok())
        .is_some_and(|m| m == "as_deref");
    if !is_as_deref {
        return false;
    }
    func.child_by_field_name("value")
        .is_some_and(|recv| is_env_var_call(recv, source))
}

/// True when `node` is `Ok("<literal>")` — a call to the `Ok` variant (bare
/// or path-qualified) with a single string-literal argument.
fn is_ok_string_literal(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let is_ok = node
        .child_by_field_name("function")
        .and_then(|f| f.utf8_text(source).ok())
        .and_then(|name| name.rsplit("::").next())
        == Some("Ok");
    if !is_ok {
        return false;
    }
    let Some(args) = node.child_by_field_name("arguments") else {
        return false;
    };
    args.named_child_count() == 1
        && args
            .named_child(0)
            .is_some_and(|arg| arg.kind() == "string_literal")
}

/// True when `node` is a call whose callee path ends in `env::var` or
/// `env::var_os` — i.e. the final segment is `var`/`var_os` and the
/// segment before it is `env` (matches `std::env::var_os`, `env::var`, …).
fn is_env_var_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "scoped_identifier" {
        return false;
    }
    let Some(name) = func
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
    else {
        return false;
    };
    if name != "var" && name != "var_os" {
        return false;
    }
    // The qualifier directly before `var`/`var_os` must be `env`.
    func.child_by_field_name("path")
        .is_some_and(|path| trailing_path_segment(path, source) == Some("env"))
}

/// The final segment of a path: the `name` of a `scoped_identifier`
/// (`std::env` → `env`) or the text of a bare `identifier` (`env` → `env`).
fn trailing_path_segment<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "identifier" => node.utf8_text(source).ok(),
        "scoped_identifier" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok()),
        _ => None,
    }
}

/// Extract the final segment of a simple flag reference, or `None` if
/// `cond` is not one of the accepted simple shapes.
fn flag_segment<'a>(cond: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    match cond.kind() {
        // `verbose`
        "identifier" => cond.utf8_text(source).ok(),
        // `self.verbose`, `opts.debug`
        "field_expression" => cond
            .child_by_field_name("field")
            .and_then(|f| f.utf8_text(source).ok()),
        // `self.verbose()` — only the no-argument call form.
        "call_expression" => {
            let args = cond.child_by_field_name("arguments")?;
            if args.named_child_count() != 0 {
                return None;
            }
            let func = cond.child_by_field_name("function")?;
            flag_segment(func, source)
        }
        _ => None,
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
    use std::fs;
    use tempfile::TempDir;

    fn run_on(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    /// Run on `rel_path` inside a temp crate with the given `Cargo.toml`,
    /// so the crate-shape check resolves against a controlled manifest
    /// instead of comply's own (binary-only) `Cargo.toml`.
    fn run_in_crate(cargo_toml_contents: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml_contents).unwrap();
        let src_path = dir.path().join(rel_path);
        if let Some(parent) = src_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
    }

    /// Run on `rel_path` inside a temp mdBook project: a `book.toml` marker at
    /// the project root and the source at `src/<rel_path>`, so the
    /// `in_mdbook_project` ancestor-walk finds the marker.
    fn run_in_mdbook(rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("book.toml"), "[book]\ntitle = \"Guide\"\n").unwrap();
        let src_path = dir.path().join("src").join(rel_path);
        fs::create_dir_all(src_path.parent().unwrap()).unwrap();
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
    }

    const LIB_CARGO_TOML: &str = r#"
[package]
name = "mylib"
version = "0.1.0"
edition = "2021"

[lib]
name = "mylib"
path = "src/lib.rs"
"#;

    const BIN_ONLY_CARGO_TOML: &str = r#"
[package]
name = "mytool"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "mytool"
path = "src/main.rs"
"#;

    /// starship shape: a CLI binary that carries a `[lib]` table (and a
    /// `src/lib.rs`) purely to expose internals to its own integration tests,
    /// alongside a `[[bin]]` target that is the real entry point.
    /// `is_binary_only()` is false here, yet the crate still owns its stderr.
    const CLI_WITH_LIB_CARGO_TOML: &str = r#"
[package]
name = "starship"
version = "0.1.0"
edition = "2021"

[lib]
name = "starship"
path = "src/lib.rs"

[[bin]]
name = "starship"
path = "src/main.rs"
"#;

    /// A build-time codegen library: its `[package].name` ends in `-build`, so
    /// it is consumed from a consumer's `build.rs`, where `eprintln!` to Cargo's
    /// build-output stream is the idiomatic diagnostic channel.
    const BUILD_CODEGEN_CARGO_TOML: &str = r#"
[package]
name = "grpc-protobuf-build"
version = "0.1.0"
edition = "2021"

[lib]
name = "grpc_protobuf_build"
path = "src/lib.rs"
"#;

    const CODEGEN_CARGO_TOML: &str = r#"
[package]
name = "something-codegen"
version = "0.1.0"
edition = "2021"

[lib]
name = "something_codegen"
path = "src/lib.rs"
"#;

    /// A proc-macro crate (`[lib] proc-macro = true`): its code runs at compile
    /// time during macro expansion, so `eprintln!` goes to the compiler's stderr
    /// at build time, never the shipped program's runtime stderr.
    const PROC_MACRO_CARGO_TOML: &str = r#"
[package]
name = "askama_derive"
version = "0.1.0"
edition = "2021"

[lib]
proc-macro = true
"#;

    /// An FFI bridge crate built as a C dynamic library (Python/Java bindings):
    /// `[lib] crate-type = ["cdylib"]`. Linked by a foreign runtime, never a Rust
    /// library consumer — `eprintln!` is the only error channel at the boundary.
    const CDYLIB_FFI_CARGO_TOML: &str = r#"
[package]
name = "cozo-lib-python"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]
"#;

    /// An FFI bridge crate built as a static library (Swift bindings):
    /// `[lib] crate-type = ["staticlib"]`.
    const STATICLIB_FFI_CARGO_TOML: &str = r#"
[package]
name = "cozo-lib-swift"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["staticlib"]
"#;

    /// A crate-type that mixes a Rust library target (`rlib`) with `cdylib` is
    /// still consumed as a Rust library, so it is NOT an FFI-only bridge and
    /// stays flagged.
    const CDYLIB_PLUS_RLIB_CARGO_TOML: &str = r#"
[package]
name = "mixed"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]
"#;

    /// A logging/tracing infrastructure crate: `[package].name` carries a
    /// `tracing` segment (`tracing-subscriber`). It implements the subscriber
    /// machinery itself, so it cannot route its own failures through tracing.
    const TRACING_SUBSCRIBER_CARGO_TOML: &str = r#"
[package]
name = "tracing-subscriber"
version = "0.1.0"
edition = "2021"

[lib]
name = "tracing_subscriber"
path = "src/lib.rs"
"#;

    /// An ordinary application/library that merely *depends on* `tracing` keeps
    /// a normal package name — it is not logging infrastructure and stays
    /// flagged.
    const TRACING_DEPENDENT_CARGO_TOML: &str = r#"
[package]
name = "myapp"
version = "0.1.0"
edition = "2021"

[lib]
name = "myapp"
path = "src/lib.rs"

[dependencies]
tracing = "0.1"
"#;

    #[test]
    fn flags_eprintln_in_library_file() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Regression for #5846 (leudz/shipyard
    /// `guide/master/src/going-further/custom_views_original.rs`): a `.rs` file
    /// under an mdBook documentation project (an ancestor `book.toml`) is
    /// tutorial example code, not compiled library code, so its `eprintln!`
    /// is exempt.
    #[test]
    fn allows_eprintln_in_mdbook_example() {
        let source = "fn handle_events(e: Event) { eprintln!(\"unhandled event: {:?}\", e); }";
        assert!(run_in_mdbook("going-further/custom_views_original.rs", source).is_empty());
    }

    /// Regression for #4465: `grpc-protobuf-build` (like `prost-build` /
    /// `tonic-build` / `bindgen`) is a build-time codegen library called from a
    /// consumer's `build.rs`. Its `eprintln!` forwards `protoc`'s stderr to
    /// Cargo's build output — the idiomatic build-script diagnostic channel.
    #[test]
    fn allows_eprintln_in_build_codegen_crate() {
        let source = "fn f() { eprintln!(\"{}\", msg); }";
        assert!(run_in_crate(BUILD_CODEGEN_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// A `-codegen`-suffixed crate is the same category of build-time codegen
    /// library and is exempt as well.
    #[test]
    fn allows_eprintln_in_codegen_crate() {
        let source = "fn f() { eprintln!(\"{}\", msg); }";
        assert!(run_in_crate(CODEGEN_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// Regression for #6030 (djc/askama `askama_derive/src/lib.rs`): a
    /// `proc-macro = true` crate runs at compile time during macro expansion,
    /// so its `eprintln!` writes to the compiler's build-time stderr (here a
    /// user-opt-in `print` debugging attribute), not the shipped program's
    /// runtime stderr. The "consumers can't capture it at runtime" premise does
    /// not hold, so it is exempt.
    #[test]
    fn allows_eprintln_in_proc_macro_crate() {
        let source = "fn f() { eprintln!(\"{:?}\", nodes); }";
        assert!(run_in_crate(PROC_MACRO_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// Strong positive: a normal `[lib]` crate (no `proc-macro = true`) ships a
    /// runtime, so its `eprintln!` is still flagged — the exemption is
    /// proc-macro-only.
    #[test]
    fn flags_eprintln_in_normal_library_crate() {
        let source = "fn f() { eprintln!(\"{:?}\", nodes); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    #[test]
    fn flags_eprint_in_library_file() {
        let source = "fn f() { eprint!(\"oops\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Regression for #4749 (cozodb/cozo `cozo-lib-python`): a `cdylib` FFI
    /// bridge crate is linked into a Python/Java runtime that never initialises
    /// a Rust tracing subscriber. `eprintln!` is the only way to surface errors
    /// at the FFI boundary, so it is exempt.
    #[test]
    fn allows_eprintln_in_cdylib_ffi_crate() {
        let source = "fn f() { eprintln!(\"{}\", err); }";
        assert!(run_in_crate(CDYLIB_FFI_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// Regression for #4749 (cozodb/cozo `cozo-lib-swift`): a `staticlib` FFI
    /// bridge crate is the same case — exempt.
    #[test]
    fn allows_eprintln_in_staticlib_ffi_crate() {
        let source = "fn f() { eprintln!(\"{err}\"); }";
        assert!(run_in_crate(STATICLIB_FFI_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// A crate that declares both `cdylib` and `rlib` is still consumed as a
    /// Rust library by other crates, so its `eprintln!` stays flagged.
    #[test]
    fn flags_eprintln_in_cdylib_plus_rlib_crate() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(
            run_in_crate(CDYLIB_PLUS_RLIB_CARGO_TOML, "src/lib.rs", source).len(),
            1
        );
    }

    /// Regression for #4994 (tokio-rs/tracing `tracing-subscriber`): a
    /// logging/tracing infrastructure crate implements the subscriber/formatter
    /// machinery itself and cannot route its own internal failures through
    /// `tracing` (that is what has failed or would recurse). `eprintln!` is its
    /// last-resort fallback — e.g. when the formatter fails or `RUST_LOG`
    /// can't be parsed — and is exempt.
    #[test]
    fn allows_eprintln_in_logging_infra_crate() {
        let source =
            "fn f() { eprintln!(\"[tracing-subscriber] Unable to format event: {:?}\", attrs); }";
        assert!(run_in_crate(TRACING_SUBSCRIBER_CARGO_TOML, "src/fmt/fmt_layer.rs", source).is_empty());
    }

    /// The logging-infra exemption keys off the crate's *own* package name, not
    /// on a `tracing` dependency: an ordinary crate that merely depends on
    /// `tracing` is not logging infrastructure and stays flagged.
    #[test]
    fn flags_eprintln_in_crate_that_merely_depends_on_tracing() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(
            run_in_crate(TRACING_DEPENDENT_CARGO_TOML, "src/lib.rs", source).len(),
            1
        );
    }

    /// Regression for #981: a module of a binary-only crate (no `[lib]`,
    /// no `src/lib.rs`) has no library consumers — `eprintln!` is fine
    /// even outside `main.rs` / `bin/`.
    #[test]
    fn allows_eprintln_in_binary_only_crate_module() {
        let source = "fn print_help() { eprintln!(\"usage\"); }";
        assert!(run_in_crate(BIN_ONLY_CARGO_TOML, "src/session.rs", source).is_empty());
    }

    #[test]
    fn flags_eprintln_in_library_crate_module() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/util.rs", source).len(), 1);
    }

    /// A library crate (it has `src/lib.rs`) that also declares an executable
    /// target by explicit `path` in a non-standard directory. The `path` field
    /// can name a `[[bin]]`, `[[example]]`, `[[bench]]`, or `[[test]]` target —
    /// all standalone executables with their own `fn main()`.
    const LIB_WITH_EXPLICIT_TARGET_CARGO_TOML: &str = r#"
[package]
name = "smoltcp"
version = "0.1.0"
edition = "2021"

[lib]
name = "smoltcp"
path = "src/lib.rs"

[[example]]
name = "packet2pcap"
path = "utils/packet2pcap.rs"
required-features = ["std"]
"#;

    /// Regression for #4728 (smoltcp `utils/packet2pcap.rs:47`): the file is a
    /// standalone executable declared via an explicit `path` in a target table
    /// (`[[example]]`/`[[bin]]`), with its own `fn main()`. It is application
    /// code even though it lives in `utils/` (not `src/main.rs` / `src/bin/`)
    /// and the crate also ships a library — `eprintln!` belongs there.
    #[test]
    fn allows_eprintln_in_explicit_path_executable_target() {
        let source = "fn main() { eprintln!(\"{e}\"); }";
        assert!(
            run_in_crate(
                LIB_WITH_EXPLICIT_TARGET_CARGO_TOML,
                "utils/packet2pcap.rs",
                source,
            )
            .is_empty()
        );
    }

    /// The explicit-target exemption is path-scoped: a genuine library module
    /// in the same crate — not named by any target `path` — stays flagged.
    #[test]
    fn flags_eprintln_in_library_module_of_crate_with_explicit_target() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(
            run_in_crate(LIB_WITH_EXPLICIT_TARGET_CARGO_TOML, "src/wire.rs", source).len(),
            1
        );
    }

    /// Regression for #1312: starship declares a `[[bin]]` target (the
    /// `starship` CLI) alongside a `[lib]` used only to expose internals to
    /// integration tests. `eprintln!` in setup/logger code is controlled CLI
    /// error output — not a library writing to a consumer's stderr.
    #[test]
    fn allows_eprintln_in_cli_crate_with_internal_lib() {
        let source = "fn init_logger() { eprintln!(\"Unable to create log dir\"); }";
        assert!(run_in_crate(CLI_WITH_LIB_CARGO_TOML, "src/logger.rs", source).is_empty());
    }

    #[test]
    fn allows_eprintln_in_main_rs() {
        let source = "fn main() { eprintln!(\"oops\"); }";
        assert!(run_on(source, "src/main.rs").is_empty());
    }

    #[test]
    fn allows_eprintln_in_bin_dir() {
        let source = "fn main() { eprintln!(\"oops\"); }";
        assert!(run_on(source, "src/bin/tool.rs").is_empty());
    }

    /// Regression for #1310: `eprintln!` gated behind `if self.verbose() { … }`
    /// is opt-in diagnostics (polars sets the flag via `POLARS_VERBOSE`),
    /// not unconditional library noise.
    #[test]
    fn allows_eprintln_under_self_verbose_method_guard() {
        let source = "fn f(&self) { if self.verbose() { eprintln!(\"CACHE SET: {id}\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    #[test]
    fn allows_eprintln_under_bare_debug_ident_guard() {
        let source = "fn f() { if debug { eprintln!(\"trace\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    #[test]
    fn allows_eprintln_under_field_verbose_guard() {
        let source = "fn f(&self) { if self.verbose { eprintln!(\"trace\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// Regression for #3941 (uv `uv-resolver/src/error.rs:789`): an
    /// `eprintln!` gated behind `std::env::var_os(KEY).is_some()` only runs
    /// when the consumer sets the variable — opt-in diagnostics, not noise.
    #[test]
    fn allows_eprintln_under_env_var_os_is_some_guard() {
        let source =
            "pub fn f() { if std::env::var_os(\"KEY\").is_some() { eprintln!(\"x\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    #[test]
    fn allows_eprintln_under_env_var_is_ok_guard() {
        let source = "pub fn f() { if std::env::var(\"KEY\").is_ok() { eprintln!(\"x\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// The `std::` prefix is optional — `env::var_os(..)` is the same gate.
    #[test]
    fn allows_eprintln_under_bare_env_var_os_guard() {
        let source = "pub fn f() { if env::var_os(\"KEY\").is_some() { eprintln!(\"x\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// The issue's exact key shape: an associated const as the key arg.
    #[test]
    fn allows_eprintln_under_env_var_os_const_key_guard() {
        let source = "pub fn f() { if std::env::var_os(EnvVars::UV_INTERNAL__SHOW_DERIVATION_TREE).is_some() { eprintln!(\"x\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// A non-`env::var` presence check is not a runtime opt-in: a plain
    /// `.is_some()` on some other call stays flagged.
    #[test]
    fn flags_eprintln_under_non_env_is_some_guard() {
        let source = "pub fn f(o: Option<u8>) { if o.is_some() { eprintln!(\"x\"); } }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Regression for #6887 (pola-rs/polars
    /// `crates/polars-mem-engine/src/scan_predicate/functions.rs:127`): an
    /// `eprintln!` gated behind `env::var(KEY).as_deref() == Ok("1")` fires only
    /// when the consumer sets the variable to that specific value — a strictly
    /// stronger opt-in than the presence form, so it is exempt too.
    #[test]
    fn allows_eprintln_under_env_var_as_deref_eq_ok_guard() {
        let source = "pub fn f() { if std::env::var(\"POLARS_OUTPUT_SKIP_BATCH_PRED\").as_deref() == Ok(\"1\") { eprintln!(\"predicate: {}\", x); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// The gate exempts an `eprintln!` nested deeper inside the gated block —
    /// here inside a further `if config::verbose()` (a scoped call the flag-name
    /// heuristic does not recognise, so only the outer env-var gate clears it),
    /// mirroring polars `time_zone.rs:68`.
    #[test]
    fn allows_eprintln_nested_under_env_var_as_deref_eq_ok_guard() {
        let source = "pub fn g() { if std::env::var(\"POLARS_IGNORE_TZ\").as_deref() == Ok(\"1\") { if config::verbose() { eprintln!(\"WARN: {}\", err) } } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// The `!=` form of the value-equality gate is recognised the same way,
    /// with the `std::` prefix optional.
    #[test]
    fn allows_eprintln_under_env_var_as_deref_ne_ok_guard() {
        let source = "pub fn h() { if env::var(\"X\").as_deref() != Ok(\"0\") { eprintln!(\"x\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// A plain boolean condition is not an env-var (or flag) opt-in gate, so an
    /// `eprintln!` under it stays flagged.
    #[test]
    fn flags_eprintln_under_non_env_var_bool_guard() {
        let source = "fn j(cond: bool) { if cond { eprintln!(\"c\"); } }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Bare, un-gated `eprintln!` in library code stays flagged even when a
    /// flag-guarded sibling exists in the same function.
    #[test]
    fn flags_ungated_eprintln_alongside_guarded_one() {
        let source = "fn f(&self) { eprintln!(\"oops\"); if self.verbose() { eprintln!(\"dbg\"); } }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// An `if` whose condition is not a verbose-style flag does not exempt.
    #[test]
    fn flags_eprintln_under_non_flag_guard() {
        let source = "fn f(items: Vec<u8>) { if items.is_empty() { eprintln!(\"oops\"); } }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// A negated flag guard is the inverse case — output when the flag is
    /// *off* is exactly the unconditional noise the rule targets.
    #[test]
    fn flags_eprintln_under_negated_verbose_guard() {
        let source = "fn f(&self) { if !self.verbose() { eprintln!(\"oops\"); } }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// The `else` branch of a flag guard is not the gated path.
    #[test]
    fn flags_eprintln_in_else_of_verbose_guard() {
        let source =
            "fn f(&self) { if self.verbose() { let _ = 1; } else { eprintln!(\"oops\"); } }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Regression for #6668 (sharkdp/numbat `numbat/src/vm.rs:555`): a function
    /// that opens with the inverted early-return guard `if !self.debug { return; }`
    /// bails out unless debug is on, so every `eprintln!` after it is opt-in
    /// diagnostics — the control-flow-inverted twin of `if self.debug { … }`.
    #[test]
    fn allows_eprintln_after_inverted_self_debug_return_guard() {
        let source = "fn disassemble(&self) { if !self.debug { return; } eprintln!(); eprintln!(\".CONSTANTS\"); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/vm.rs", source).is_empty());
    }

    /// The bare-identifier flag form `if !verbose { return; }` is the same
    /// entry guard and exempts the following `eprintln!`.
    #[test]
    fn allows_eprintln_after_inverted_bare_verbose_return_guard() {
        let source = "fn f() { if !verbose { return; } eprintln!(\"trace\"); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// The inverted guard is recognised only on the *first* statement: an
    /// `eprintln!` preceded by ordinary code (no entry guard) stays flagged.
    #[test]
    fn flags_eprintln_without_inverted_guard() {
        let source = "fn f(&self) { let x = 1; eprintln!(\"oops\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// The flag-name gate applies to the inverted guard too: a negated
    /// non-verbose flag (`!self.ready`) is not a diagnostics opt-in, so the
    /// following `eprintln!` stays flagged.
    #[test]
    fn flags_eprintln_after_inverted_non_verbose_return_guard() {
        let source = "fn f(&self) { if !self.ready { return; } eprintln!(\"oops\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// The inverted guard must early-*return*: a first-statement `if !self.debug`
    /// whose body does something other than `return` does not divert control
    /// flow, so the following unconditional `eprintln!` stays flagged.
    #[test]
    fn flags_eprintln_after_inverted_guard_without_return() {
        let source = "fn f(&self) { if !self.debug { do_thing(); } eprintln!(\"oops\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Regression for #5197 (zkat/miette `src/panic.rs:24`): an `eprintln!`
    /// inside the closure passed to `std::panic::set_hook(Box::new(…))` is a
    /// panic hook writing the crash report to stderr — its intended job. The
    /// `tracing` alternative is unreliable mid-panic, so it is exempt.
    #[test]
    fn allows_eprintln_in_panic_hook_closure() {
        let source = "pub fn set_panic_hook() { std::panic::set_hook(Box::new(move |info| { eprintln!(\"Error: {:?}\", info); })); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/panic.rs", source).is_empty());
    }

    /// A `set_hook` closure passed directly (no `Box::new` wrapper) is the
    /// same case.
    #[test]
    fn allows_eprintln_in_bare_set_hook_closure() {
        let source = "pub fn f() { panic::set_hook(|info| { eprintln!(\"{info}\"); }); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// The exemption is scoped to the hook closure: an `eprintln!` in a
    /// *different* closure nested inside the hook body resolves to that inner
    /// closure (not a `set_hook` argument) and stays flagged.
    #[test]
    fn flags_eprintln_in_nested_non_hook_closure_inside_panic_hook() {
        let source = "pub fn f() { std::panic::set_hook(Box::new(|_info| { (|| { eprintln!(\"oops\"); })(); })); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// A closure passed to some other (non-`set_hook`) call is not a panic
    /// hook — ordinary library `eprintln!` in such a closure stays flagged.
    #[test]
    fn flags_eprintln_in_non_set_hook_closure() {
        let source = "pub fn f() { with_thing(Box::new(|info| { eprintln!(\"{info}\"); })); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Regression for #6836 (nushell/nushell
    /// `crates/nu-cmd-lang/src/core_commands/if_.rs:112`): an `eprintln!`
    /// immediately followed by `unreachable!()` (the block's trailing
    /// expression, no `;`) is a pre-panic diagnostic — the process terminates
    /// on the next statement, so there is nothing to redirect. Exempt.
    #[test]
    fn allows_eprintln_immediately_before_unreachable() {
        let source = "fn run(&self) { eprintln!(\"this code path should never be reached\"); unreachable!() }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// The same exemption covers a following `panic!(…)` — also a guaranteed
    /// unconditional termination on the next statement.
    #[test]
    fn allows_eprintln_immediately_before_panic() {
        let source = "fn f() { eprintln!(\"fatal: bad state\"); panic!(\"bad state\"); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// The terminator is matched on the macro's final path segment, so a
    /// path-qualified `std::unreachable!()` / `core::panic!()` is recognised
    /// too — the preceding `eprintln!` is exempt.
    #[test]
    fn allows_eprintln_before_path_qualified_unreachable() {
        let source = "fn f() { eprintln!(\"x\"); std::unreachable!() }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// A bare `eprintln!` that is the last statement of its block — with no
    /// following panic — is unconditional library output and stays flagged
    /// (must not panic on the missing next sibling).
    #[test]
    fn flags_eprintln_as_last_statement_with_no_panic() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// An `eprintln!` whose following statement is ordinary code — not an
    /// `unreachable!`/`panic!` — is not a pre-panic diagnostic and stays
    /// flagged.
    #[test]
    fn flags_eprintln_followed_by_ordinary_statement() {
        let source = "fn f() { eprintln!(\"oops\"); do_more(); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Regression for #7605 (lapce/lapce `lapce-app/src/app.rs:3794`): an
    /// `eprintln!` immediately followed by `std::process::exit(1)` is a
    /// pre-termination diagnostic — the process terminates unconditionally on
    /// the next statement, so there is nothing left for a consumer to redirect
    /// or capture, the same category as the pre-`panic!` exemption. Exempt.
    #[test]
    fn allows_eprintln_immediately_before_process_exit() {
        let source = "fn f() { eprintln!(\"Failed to launch: {why}\"); std::process::exit(1); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/app.rs", source).is_empty());
    }

    /// The `std::` prefix is optional: a `process::exit(1)` call is recognised
    /// the same way — final segment `exit` qualified by `process`.
    #[test]
    fn allows_eprintln_immediately_before_bare_qualified_process_exit() {
        let source = "fn f() { eprintln!(\"bye\"); process::exit(1); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// `std::process::abort()` is the same unconditional termination as `exit`
    /// and exempts the preceding `eprintln!` too.
    #[test]
    fn allows_eprintln_immediately_before_process_abort() {
        let source = "fn f() { eprintln!(\"fatal\"); std::process::abort(); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// The pre-termination exemption is the IMMEDIATELY-following statement
    /// only: an `eprintln!` separated from a later `process::exit` by ordinary
    /// code is unconditional library output at the point it runs and stays
    /// flagged.
    #[test]
    fn flags_eprintln_with_process_exit_after_other_statements() {
        let source = "fn f() { eprintln!(\"oops\"); do_more(); std::process::exit(1); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// The call form requires the `process` qualifier: an unqualified `exit(…)`
    /// (a local function, not `std::process::exit`) is not a recognised
    /// terminator, so the preceding `eprintln!` stays flagged. This keeps the
    /// exemption a structural `process::exit` match, not a bare-name allowlist.
    #[test]
    fn flags_eprintln_before_unqualified_exit_call() {
        let source = "fn f() { eprintln!(\"oops\"); exit(1); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// The qualifier must be `process`: a scoped `exit` under a different path
    /// (`libc::exit`) is not the std process terminator this exemption covers,
    /// so the preceding `eprintln!` stays flagged.
    #[test]
    fn flags_eprintln_before_non_process_qualified_exit_call() {
        let source = "fn f() { eprintln!(\"oops\"); libc::exit(1); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Regression for #5641 (foundry-rs/foundry `crates/cli/src/opts/global.rs`):
    /// an `eprintln!` inside a `#[allow(clippy::disallowed_macros)] { … }` block
    /// is covered by the author's explicit suppression of clippy's
    /// `disallowed_macros` lint (which bans `eprintln!`). Honoring that scoped
    /// attribute exempts the macro, just as comply does for other
    /// clippy-mirroring rules.
    #[test]
    fn allows_eprintln_under_clippy_disallowed_macros_allow() {
        let source = "pub fn f() { #[allow(clippy::disallowed_macros)] { eprintln!(\"note\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// The `#[expect(...)]` form scoped to the block is honored too.
    #[test]
    fn allows_eprintln_under_clippy_disallowed_macros_expect() {
        let source = "pub fn f() { #[expect(clippy::disallowed_macros)] { eprintln!(\"note\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// The attribute exemption is specific to `clippy::disallowed_macros`: an
    /// `#[allow]` for an unrelated lint does not suppress the diagnostic.
    #[test]
    fn flags_eprintln_under_unrelated_clippy_allow() {
        let source = "pub fn f() { #[allow(clippy::print_stderr)] { eprintln!(\"oops\"); } }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Case (1) of #5641 (foundry-rs/foundry `crates/config/src/lib.rs`): an
    /// `eprintln!` documented only by a `//` comment — with NO suppression
    /// attribute — stays flagged. A comment is not a suppression signal; the
    /// dev can add `#[allow(clippy::disallowed_macros)]` to exempt it.
    #[test]
    fn flags_eprintln_with_explanatory_comment_only() {
        let source = "pub fn f() {\n    // `sh_warn!` is a circular dependency, preventing us from using it here.\n    eprintln!(\"oops\");\n}";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Regression for #4474 (anyhow `build.rs`): a Cargo build script is a
    /// separate binary run at build time, not library code. `eprintln!` there
    /// writes to Cargo's build-output stream — the idiomatic diagnostic channel.
    /// Run inside a library-only crate so the build-script exemption (not a
    /// `[[bin]]`/codegen manifest) is the only thing that can clear it.
    #[test]
    fn allows_eprintln_in_build_script() {
        let source = "fn main() { eprintln!(\"Failed: {}\", err); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "build.rs", source).is_empty());
    }

    #[test]
    fn allows_eprintln_in_test_function() {
        let source = "#[test]\nfn t() { eprintln!(\"trace\"); }";
        assert!(run_on(source, "src/lib.rs").is_empty());
    }

    #[test]
    fn allows_eprintln_in_tests_dir() {
        let source = "fn f() { eprintln!(\"trace\"); }";
        assert!(run_on(source, "tests/it.rs").is_empty());
    }

    /// Regression for #5594 (qdrant `lib/segment/src/payload_storage/tests.rs`):
    /// a `src/**/tests.rs` file is the idiomatic Rust inline-test-module
    /// (`mod tests;` -> sibling `tests.rs`). A helper called from its `#[test]`
    /// functions carries no `#[test]`/`#[cfg(test)]` attribute on the call site,
    /// so the attribute walk misses it — the filename marks it as test code.
    /// Run inside a library crate so only the test-file gate can clear it.
    #[test]
    fn allows_eprintln_in_inline_tests_module_file() {
        let source = "fn helper() { eprintln!(\"storage is correct before drop\"); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/payload_storage/tests.rs", source).is_empty());
    }

    /// The singular `test.rs` inline-test-module name is the same convention.
    #[test]
    fn allows_eprintln_in_singular_test_module_file() {
        let source = "fn helper() { eprintln!(\"trace\"); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/parser/test.rs", source).is_empty());
    }

    /// The test-file gate matches the whole filename stem `tests`/`test` only: a
    /// library file whose stem merely *contains* `test` (`latest.rs`) is
    /// production code and stays flagged.
    #[test]
    fn flags_eprintln_in_latest_rs_library_file() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/latest.rs", source).len(), 1);
    }

    /// Regression for #6327 (toml-rs/toml `toml_parser/src/debug.rs:27`):
    /// `anstream::eprintln!` is the `anstream` crate's library-friendly,
    /// stream-redirectable macro — a different macro from `std::eprintln!`.
    /// A path-qualified invocation from a non-`std`/`core` crate is exempt.
    #[test]
    fn allows_path_qualified_anstream_eprintln() {
        let source = "pub(crate) fn trace() { anstream::eprintln!(\"{}\", x); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// Any other non-`std`/`core` qualifier names a different crate's macro and
    /// is exempt as well — the test is on the path prefix, not an allowlist.
    #[test]
    fn allows_path_qualified_third_party_eprint() {
        let source = "fn f() { mycrate::eprint!(\"x\"); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// Precision: an explicit `std::` qualifier is the std macro and still fires.
    #[test]
    fn flags_std_qualified_eprintln() {
        let source = "fn f() { std::eprintln!(\"x\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Precision: a `core::` qualifier is the std macro re-export and still fires.
    #[test]
    fn flags_core_qualified_eprint() {
        let source = "fn f() { core::eprint!(\"x\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Precision: a leading global-path `::std::` qualifier is still the std
    /// macro and fires — the leading `::` is normalized before the prefix test.
    #[test]
    fn flags_global_path_std_eprintln() {
        let source = "fn f() { ::std::eprintln!(\"x\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }
}
