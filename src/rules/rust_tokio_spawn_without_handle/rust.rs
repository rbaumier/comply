//! rust-tokio-spawn-without-handle backend.
//!
//! Walks `expression_statement` nodes and flags any whose direct
//! child is a `call_expression` to `tokio::spawn` (or any path
//! ending in `::spawn` from the tokio family). The fact that the
//! call sits at expression-statement level means its return value
//! — the JoinHandle — is being dropped, which is the bug.
//!
//! A bare `spawn` is treated as tokio only when no `use std::thread::spawn`
//! brings it into scope from `std::thread`; that API is fire-and-forget by
//! design and dropping its `JoinHandle` is idiomatic.
//!
//! A spawn statement inside a loop body (`for`/`while`/`loop`) is exempt. The
//! motivating case is the tokio accept-loop idiom — one task spawned per
//! incoming connection, where retaining the handles would require an unbounded
//! `Vec<JoinHandle>` for no benefit — but the exemption covers any spawn in a
//! loop body, since a handle created per iteration cannot meaningfully be held.
//!
//! A spawn whose task body contains a process-termination primitive
//! (`process::exit(..)` / `process::abort(..)`) is exempt. Such a task provably
//! never returns — the call ends the whole process — so its `JoinHandle` can
//! never resolve and capturing it would be dead code (the ctrl-c / shutdown
//! handler idiom).
//!
//! Three exemptions therefore suppress the diagnostic: a test context
//! (`#[test]`/`#[tokio::test]`/`#[cfg(test)]`), a spawn in a loop body, and a
//! spawned task that terminates the process.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{is_in_loop_body, is_in_test_context};

const KINDS: &[&str] = &["expression_statement"];

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
        // The expression-statement wraps a single expression.
        // For `foo();` that's the `call_expression`.
        let Some(call) = node.named_child(0) else {
            return;
        };
        if call.kind() != "call_expression" {
            return;
        }
        let Some(function) = call.child_by_field_name("function") else {
            return;
        };
        let Ok(text) = function.utf8_text(source_bytes) else {
            return;
        };
        if !is_tokio_spawn(text, node, source_bytes) {
            return;
        }
        if is_in_test_context(node, source_bytes) {
            return;
        }
        // Spawn inside a loop body (the per-connection accept-loop idiom):
        // a handle created each iteration cannot meaningfully be retained, so
        // dropping it is intentional. Stops at the fn/closure boundary so only
        // a loop in this scope counts.
        if is_in_loop_body(node) {
            return;
        }
        // A spawned task that calls a process-termination primitive
        // (`process::exit`/`process::abort`) never returns — the call ends the
        // whole process — so the `JoinHandle` can never resolve and capturing it
        // would be dead code (the ctrl-c / shutdown handler idiom).
        if let Some(arguments) = call.child_by_field_name("arguments")
            && spawned_body_terminates_process(arguments, source_bytes)
        {
            return;
        }
        let pos = call.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-tokio-spawn-without-handle".into(),
            message: format!(
                "`{text}(..)` discards its `JoinHandle` — panics in \
                 the spawned task are silently swallowed. Capture the \
                 handle and `.await` it, or wrap the work in a \
                 logging helper."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True if `text` is a `tokio::spawn` call. Qualified calls
/// (`tokio::spawn`, `tokio::task::spawn`) match by suffix. A bare `spawn`
/// is ambiguous — it could be `tokio::spawn` or `std::thread::spawn`,
/// both imported as `use …::spawn` and called unqualified — so it only
/// counts as tokio when the file's `use` declarations do not bring
/// `spawn` in from `std::thread` (`std::thread::spawn` is fire-and-forget
/// by design and discarding its `JoinHandle` is idiomatic).
fn is_tokio_spawn(text: &str, node: tree_sitter::Node, source: &[u8]) -> bool {
    if text == "tokio::spawn" || text == "tokio::task::spawn" || text.ends_with("::tokio::spawn") {
        return true;
    }
    text == "spawn" && !spawn_imported_from_std_thread(node, source)
}

/// True if the spawned-task argument subtree contains a call to a process-
/// termination primitive (`process::exit(..)` / `process::abort(..)`). Such a
/// task never returns — the call terminates the whole process — so its
/// `JoinHandle` can never resolve and capturing it would be dead code. Walks the
/// whole subtree (the issue's ctrl-c handler reaches the exit via a tail
/// statement, but a conditional exit is covered too).
fn spawned_body_terminates_process(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "call_expression" && call_terminates_process(node, source) {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| spawned_body_terminates_process(child, source))
}

/// True if `call` is `process::exit(..)` or `process::abort(..)` — a
/// `call_expression` whose `function` is a `scoped_identifier` whose final
/// segment is `exit`/`abort` and whose preceding segment is `process`. Matching
/// the two-segment `process::exit`/`process::abort` suffix covers
/// `std::process::exit`, `process::exit` (via `use std::process;`), and
/// `::std::process::abort`, while a bare `exit()` or an unrelated method
/// (`menu.exit()`) does not match.
fn call_terminates_process(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "scoped_identifier" {
        return false;
    }
    let Ok(text) = function.utf8_text(source) else {
        return false;
    };
    let mut segments = text.rsplit("::").map(str::trim);
    let Some(last) = segments.next() else {
        return false;
    };
    if last != "exit" && last != "abort" {
        return false;
    }
    segments.next() == Some("process")
}

/// True when the file has a `use` declaration that brings the bare
/// identifier `spawn` into scope from `std::thread` (e.g.
/// `use std::thread::spawn;` or `use std::thread::{sleep, spawn};`).
fn spawn_imported_from_std_thread(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    find_std_thread_spawn_import(root, source)
}

fn find_std_thread_spawn_import(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "use_declaration"
        && let Ok(text) = node.utf8_text(source)
        && use_imports_spawn_from_std_thread(text)
    {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if find_std_thread_spawn_import(child, source) {
            return true;
        }
    }
    false
}

/// True when a `use` declaration imports the bare `spawn` from a `thread`
/// module (`std::thread::spawn`, or a re-export path ending in
/// `thread::spawn`). Handles single (`use std::thread::spawn;`), grouped
/// (`use std::thread::{sleep, spawn};`), and nested-group
/// (`use std::{thread::spawn, sync::Arc};`) imports. An `as` alias rebinds
/// `spawn` to another name, so it no longer matches.
fn use_imports_spawn_from_std_thread(use_text: &str) -> bool {
    let Some(path) = strip_use_prefix(use_text) else {
        return false;
    };
    match path.split_once('{') {
        Some((prefix, group)) => {
            let prefix = prefix.trim();
            group
                .trim_end_matches(['}', ';'])
                .split(',')
                .any(|member| leaf_is_thread_spawn(prefix, member.trim()))
        }
        None => leaf_is_thread_spawn("", path),
    }
}

/// True when joining `prefix` (the path up to a group, or empty for a single
/// import) with `member` yields a `thread::spawn` import — i.e. the leaf is
/// the bare `spawn` and the module path passes through `thread`. An `as`
/// alias rebinds the name, so it never matches.
fn leaf_is_thread_spawn(prefix: &str, member: &str) -> bool {
    if member.contains(" as ") {
        return false;
    }
    let full = format!("{}{}", prefix, member);
    full == "std::thread::spawn" || full == "thread::spawn" || full.ends_with("::thread::spawn")
}

/// Strip a leading `pub`/`pub(...)` and `use`, plus a trailing `;`,
/// returning the import path. `None` if the text is not a `use` item.
fn strip_use_prefix(use_text: &str) -> Option<&str> {
    let trimmed = use_text.trim_start();
    let after_pub = trimmed
        .strip_prefix("pub(crate)")
        .or_else(|| trimmed.strip_prefix("pub(super)"))
        .or_else(|| trimmed.strip_prefix("pub"))
        .unwrap_or(trimmed)
        .trim_start();
    let rest = after_pub.strip_prefix("use")?;
    Some(rest.trim().trim_end_matches(';').trim())
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
    fn flags_fire_and_forget_spawn() {
        let source = "fn f() { tokio::spawn(async { work().await }); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_spawn_assigned_to_variable() {
        let source = "fn f() { let h = tokio::spawn(async { work().await }); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_spawn_returned() {
        let source = "fn f() -> JoinHandle<()> { tokio::spawn(async { work().await }) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_other_calls() {
        let source = "fn f() { other_function(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_spawn_in_tokio_test() {
        let source = r#"
#[tokio::test]
async fn my_test() {
    tokio::spawn(async move { work().await });
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_spawn_in_plain_test() {
        let source = r#"
#[test]
fn my_test() {
    tokio::spawn(async { work().await });
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_spawn_in_cfg_test_module() {
        let source = r#"
#[cfg(test)]
mod tests {
    fn helper() {
        tokio::spawn(async { work().await });
    }
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_spawn_in_production_code() {
        let source = r#"
fn start_worker() {
    tokio::spawn(async { process().await });
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    // --- #1321: canonical detached/loop spawns are exempt ---

    #[test]
    fn allows_per_connection_spawn_in_accept_loop() {
        // mini-redis src/server.rs:259 — one spawn per incoming connection.
        let source = r#"
async fn run(&self) -> Result<()> {
    loop {
        let (socket, _) = self.listener.accept().await?;
        tokio::spawn(async move {
            if let Err(err) = handler.run().await {
                error!(cause = ?err, "connection error");
            }
            drop(permit);
        });
    }
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_per_connection_spawn_in_while_let_accept_loop() {
        let source = r#"
async fn run(&self) {
    while let Ok((socket, _)) = self.listener.accept().await {
        tokio::spawn(handle(socket));
    }
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_let_underscore_bound_spawn() {
        // The rule keys on `expression_statement`. `let _ = tokio::spawn(...)`
        // parses as a `let_declaration`, so it is outside the rule's scope and
        // is not reported (this is a suppression form, not a behavioral change:
        // `let _` still drops the handle).
        let source = r#"
fn new() -> Db {
    let _ = tokio::spawn(purge_expired_tasks(shared.clone()));
    Db { shared }
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_bare_background_spawn_statement() {
        // mini-redis src/db.rs:135 — a bare `tokio::spawn(named_task())`
        // statement outside a loop drops its handle and swallows the task's
        // panics, so it is a true positive and still fires.
        let source = r#"
fn new() -> Db {
    let shared = Arc::new(SharedState::new());
    tokio::spawn(purge_expired_tasks(shared.clone()));
    Db { shared }
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_inline_async_block_inside_loop_is_still_exempt() {
        // A loop-body spawn with an inline async block is the accept-loop
        // shape: exempt by the loop signal even though the arg is inline.
        let source = r#"
async fn serve(&self) {
    loop {
        tokio::spawn(async move { handle().await });
    }
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_inline_async_block_in_sequential_body() {
        // Not a loop, not a named-call task: a genuine fire-and-forget leak in
        // a normal sequential body — must STILL fire.
        let source = r#"
async fn handle_request(&self) {
    self.prepare().await;
    tokio::spawn(async move { send_metrics().await });
    self.respond().await;
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_inline_async_block_in_fn_returning_value() {
        // A value-returning fn does not by itself license dropping the handle:
        // an inline async block here is still an ad-hoc leak.
        let source = r#"
fn build() -> Config {
    let cfg = load();
    tokio::spawn(async move { do_unrelated_work().await });
    cfg
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_spawn_of_closure_in_sequential_body() {
        // A closure argument is also ad-hoc fire-and-forget — keep firing.
        let source = r#"
fn start() {
    tokio::spawn(|| work());
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_named_call_spawn_in_sequential_body() {
        // A named-call future argument outside a loop is not special: its
        // handle is dropped and panics are swallowed just like an inline block.
        let source = r#"
async fn handle_request(&self) {
    self.prepare().await;
    tokio::spawn(send_metrics());
    self.respond().await;
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    // --- #4715: bare `spawn` imported from std::thread is not tokio ---

    #[test]
    fn allows_bare_spawn_imported_from_std_thread() {
        // tungstenite-rs tests/connection_reset.rs:25 — `use std::thread::spawn`
        // then a bare `spawn(|| { … })` watchdog thread. Detaching the
        // JoinHandle is idiomatic for std::thread, not a tokio leak.
        let source = r#"
use std::thread::{sleep, spawn};

fn do_test() {
    spawn(|| {
        sleep(Duration::from_secs(5));
        exit(1);
    });
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_bare_spawn_from_single_std_thread_import() {
        let source = r#"
use std::thread::spawn;

fn start() {
    spawn(|| work());
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_bare_spawn_imported_from_tokio() {
        // `use tokio::spawn` then bare `spawn(...)` — still a dropped tokio
        // JoinHandle, must keep firing.
        let source = r#"
use tokio::spawn;

fn start_worker() {
    spawn(async { process().await });
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_bare_spawn_with_no_std_thread_import() {
        // No `use std::thread::spawn` in scope: the bare `spawn` defaults to
        // the tokio interpretation and fires.
        let source = r#"
fn start_worker() {
    spawn(async { process().await });
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_bare_spawn_from_nested_std_group_import() {
        // `use std::{thread::spawn, sync::Arc};` — spawn comes from std::thread
        // via a nested group; still not a tokio call.
        let source = r#"
use std::{thread::spawn, sync::Arc};

fn start() {
    spawn(|| work());
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_bare_spawn_when_std_thread_spawn_is_aliased() {
        // `use std::thread::spawn as thread_spawn;` rebinds the std import, so a
        // bare `spawn` no longer refers to it and defaults to tokio.
        let source = r#"
use std::thread::spawn as thread_spawn;

fn start_worker() {
    spawn(async { process().await });
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    // --- #6946: spawned process-termination handler is exempt ---

    #[test]
    fn allows_spawn_that_calls_std_process_exit() {
        // meilisearch crates/meilisearch/src/main.rs:136 — a ctrl-c handler that
        // exits the process. The task never returns, so its JoinHandle is unusable.
        let source = r#"
fn main() {
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        std::process::exit(130);
    });
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_spawn_that_calls_std_process_abort() {
        let source = r#"
fn main() {
    tokio::spawn(async move {
        watchdog().await;
        std::process::abort();
    });
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_spawn_with_process_exit_via_use_std_process() {
        // `use std::process;` then a `process::exit(1)` — the two-segment suffix
        // match covers the unqualified-module form.
        let source = r#"
use std::process;

fn main() {
    tokio::spawn(async move {
        wait_for_shutdown().await;
        process::exit(1);
    });
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_spawn_without_termination_call() {
        // An ordinary fire-and-forget spawn with no process-termination call is
        // still a dropped JoinHandle.
        let source = r#"
fn start() {
    tokio::spawn(async move { do_work().await; });
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_spawn_with_unrelated_exit_method_call() {
        // `menu.exit()` is a method named `exit`, not `process::exit` — the
        // exemption must not catch it, so the dropped handle still fires.
        let source = r#"
fn start() {
    tokio::spawn(async move {
        let menu = build_menu();
        menu.exit();
    });
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_spawn_with_unrelated_exit_code_method_call() {
        // `self.exit_code()` is an unrelated method, not a process-termination
        // primitive — still a dropped handle.
        let source = r#"
fn start(&self) {
    tokio::spawn(async move {
        let code = self.exit_code();
        report(code).await;
    });
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }
}
