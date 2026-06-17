//! rust-tokio-spawn-without-handle backend.
//!
//! Walks `expression_statement` nodes and flags any whose direct
//! child is a `call_expression` to `tokio::spawn` (or any path
//! ending in `::spawn` from the tokio family). The fact that the
//! call sits at expression-statement level means its return value
//! — the JoinHandle — is being dropped, which is the bug.
//!
//! A spawn statement inside a loop body (`for`/`while`/`loop`) is exempt. The
//! motivating case is the tokio accept-loop idiom — one task spawned per
//! incoming connection, where retaining the handles would require an unbounded
//! `Vec<JoinHandle>` for no benefit — but the exemption covers any spawn in a
//! loop body, since a handle created per iteration cannot meaningfully be held.

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
        if !is_tokio_spawn(text) {
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

/// True if `text` looks like a `tokio::spawn` call by suffix match.
/// Accepts `tokio::spawn`, `tokio::task::spawn`, and bare `spawn`
/// (when paired with a `use tokio::spawn`).
fn is_tokio_spawn(text: &str) -> bool {
    text == "tokio::spawn"
        || text == "tokio::task::spawn"
        || text.ends_with("::tokio::spawn")
        || text == "spawn"
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
}
