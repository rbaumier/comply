//! axum-serve-no-graceful-shutdown backend.
//!
//! The axum entrypoint `axum::serve(listener, app)` returns a `Serve` future.
//! Awaiting it directly serves until the process is killed and, on
//! `SIGTERM`/`SIGINT`, drops every in-flight request. Converting it with
//! `.with_graceful_shutdown(signal)` drains open connections before exit.
//!
//! Detection is a single `call_expression` shape:
//!
//! - `function` is the fully-qualified `scoped_identifier` `axum::serve` (path
//!   `axum`, name `serve`) — the real axum 0.7+ entrypoint. A bare `serve(...)`
//!   or a `.serve(...)` method on some other builder is too generic to key on
//!   and stays silent.
//! - The call is flagged unless it is the receiver of a directly-chained
//!   `.with_graceful_shutdown(...)` — i.e. its parent is a `field_expression`
//!   whose field is `with_graceful_shutdown`. `Serve` exposes exactly that one
//!   method to install a shutdown signal, so the immediate-parent check is
//!   exact and cannot miss a guarded chain.
//! - A `Serve` stored in a `let` binding is left silent: the bound variable may
//!   receive `.with_graceful_shutdown(...)` later, which a syntax-only check
//!   cannot follow, so the rule prefers silence over a false positive.

use crate::diagnostic::{Diagnostic, Severity};

/// The `name` segment of a `scoped_identifier` (`axum::serve` -> `serve`).
fn segment_name<'a>(scoped: tree_sitter::Node<'a>, source: &'a [u8]) -> &'a str {
    scoped
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("")
}

/// `axum::serve` — a `scoped_identifier` with path `axum` and name `serve`.
fn is_axum_serve(func: tree_sitter::Node, source: &[u8]) -> bool {
    func.kind() == "scoped_identifier"
        && segment_name(func, source) == "serve"
        && func.child_by_field_name("path").is_some_and(|p| {
            p.kind() == "identifier" && p.utf8_text(source).unwrap_or("") == "axum"
        })
}

/// The `axum::serve(...)` call is the receiver of a directly-chained
/// `.with_graceful_shutdown(...)` — the only `Serve` method that installs a
/// shutdown signal.
fn has_graceful_shutdown(anchor: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(parent) = anchor.parent() else {
        return false;
    };
    parent.kind() == "field_expression"
        && parent.child_by_field_name("value").map(|v| v.id()) == Some(anchor.id())
        && parent
            .child_by_field_name("field")
            .and_then(|f| f.utf8_text(source).ok())
            == Some("with_graceful_shutdown")
}

/// The `axum::serve(...)` result is stored in a `let` binding, taking it out of
/// the rule's sight — the bound variable may receive `.with_graceful_shutdown`
/// later, which a syntax-only check cannot follow.
fn is_bound_to_variable(anchor: tree_sitter::Node) -> bool {
    anchor.parent().is_some_and(|p| p.kind() == "let_declaration")
}

crate::ast_check! { on ["call_expression"] prefilter = ["axum::serve"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if !is_axum_serve(func, source) { return; }
    if has_graceful_shutdown(node, source) { return; }
    if is_bound_to_variable(node) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`axum::serve(...)` without `.with_graceful_shutdown(...)` drops in-flight \
         requests on SIGTERM/SIGINT. Chain \
         `.with_graceful_shutdown(shutdown_signal())` to drain open connections \
         before exit."
            .into(),
        Severity::Error,
    ));
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    // ── Positive: the entrypoint without a graceful-shutdown chain ─────────

    #[test]
    fn flags_axum_serve_awaited_directly() {
        let src = "async fn main() { axum::serve(listener, app).await.unwrap(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_axum_serve_with_try_operator() {
        let src = "async fn run() -> Result<(), Error> { axum::serve(listener, app).await?; Ok(()) }";
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: guarded, bound, or unrelated shapes stay silent ──────────

    #[test]
    fn allows_axum_serve_with_graceful_shutdown() {
        let src = "async fn main() { axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await.unwrap(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_let_bound_serve_with_deferred_shutdown() {
        // `Serve` bound to a variable, shutdown applied through the binding: a
        // syntax-only check cannot follow the variable, so the rule stays silent
        // rather than false-positive on this safe (if non-idiomatic) form.
        let src = "async fn main() { let server = axum::serve(listener, app); server.with_graceful_shutdown(shutdown_signal()).await.unwrap(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_serve_call() {
        // A bare `serve(...)` (not the `axum::serve` path) is too generic to key on.
        let src = "async fn main() { serve(listener, app).await.unwrap(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_serve_method_on_other_builder() {
        // `.serve(...)` on some other type is not the axum free function.
        let src = "async fn main() { builder.serve(app).await.unwrap(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unrelated_module_serve() {
        // `other::serve(...)` is not `axum::serve(...)`.
        let src = "async fn main() { other::serve(listener, app).await.unwrap(); }";
        assert!(run(src).is_empty());
    }
}
