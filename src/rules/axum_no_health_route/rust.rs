//! axum-no-health-route backend.
//!
//! An `axum::serve(listener, app)` entrypoint serves a `Router`. Without a
//! health-check route (`/health`, `/healthz`, …), an orchestrator or load
//! balancer has no endpoint to probe for liveness/readiness.
//!
//! Proving the *absence* of a health route across a whole app is
//! false-positive-prone, so detection stays deliberately narrow: it fires only
//! when the router is assembled directly in the serving file and every route is
//! visible to this syntax-only check.
//!
//! Fired at a single `call_expression` — the real axum entrypoint
//! `axum::serve` (a `scoped_identifier` with path `axum`, name `serve`) — and
//! only when ALL of:
//!
//! - the file constructs a router in place (`Router::new` appears in source), so
//!   the routing surface is defined here rather than imported wholesale from a
//!   builder in another module;
//! - at least one route is registered directly via `.route(` in this file, so
//!   the set of paths is fully visible;
//! - no route path matches a health pattern anywhere in the file; and
//! - the router composes no sub-routers via `.merge(` / `.nest(` /
//!   `.nest_service(`, whose routes this check cannot see (a merged/nested
//!   sub-router may itself carry the health route).
//!
//! Any of those escape hatches leaves the rule silent — it prefers a missed
//! violation over a false positive on idiomatic safe code.

use crate::diagnostic::{Diagnostic, Severity};

/// Route-path substrings that mark a liveness/readiness probe. A literal match
/// anywhere in the file (route path, constant, string) silences the rule. Each
/// leading-slash needle substring-covers its suffixed variants, so only the
/// distinct roots are listed: `/health` -> `/healthz`, `/health/live`,
/// `/api/health`; `/live` -> `/livez`, `/liveness`; `/ready` -> `/readyz`. The
/// leading slash keeps the match route-anchored (`/delivery`, `/shipping`,
/// `/already` are not silenced). The list covers the common orchestrator probe
/// conventions; a probe registered under an unrecognized custom path (e.g.
/// `/status`, `/up`) is not matched, so the rule may still fire there — an
/// accepted, narrow residual.
const HEALTH_PATHS: &[&str] = &[
    "/health",
    "/live",
    "/ready",
    "/readiness",
    "/ping",
    "/heartbeat",
    "/_health",
];

/// The `name` segment of a `scoped_identifier` (`axum::serve` -> `serve`).
fn segment_name<'a>(scoped: tree_sitter::Node<'a>, source: &'a [u8]) -> &'a str {
    scoped
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("")
}

/// `axum::serve` — a `scoped_identifier` with path `axum` and name `serve`.
/// A bare `serve(...)` or a `.serve(...)` method on some other builder is too
/// generic to key on and stays silent.
fn is_axum_serve(func: tree_sitter::Node, source: &[u8]) -> bool {
    func.kind() == "scoped_identifier"
        && segment_name(func, source) == "serve"
        && func.child_by_field_name("path").is_some_and(|p| {
            p.kind() == "identifier" && p.utf8_text(source).unwrap_or("") == "axum"
        })
}

crate::ast_check! { on ["call_expression"] prefilter = ["axum::serve"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if !is_axum_serve(func, source) { return; }

    // Only judge a router whose routes are fully visible in this file: it is
    // constructed here (`Router::new`) and its paths are registered directly
    // (`.route(`). A router built by a helper in another module, or one that
    // composes sub-routers via `.merge(`/`.nest(`/`.nest_service(`, may
    // register the health route out of this check's sight — absence is then
    // unprovable, so stay silent rather than false-positive.
    if !ctx.source_contains("Router::new") { return; }
    if !ctx.source_contains(".route(") { return; }
    if ctx.source_contains(".merge(")
        || ctx.source_contains(".nest(")
        || ctx.source_contains(".nest_service(")
    {
        return;
    }

    // A health route registered anywhere in this file satisfies the probe.
    if HEALTH_PATHS.iter().any(|p| ctx.source_contains(p)) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`axum::serve(...)` serves a `Router` with no `/health` route — \
         orchestrators and load balancers have no liveness probe. Register \
         `.route(\"/health\", get(...))` on the router before serving it."
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

    fn run_at(s: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, s, path)
    }

    // ── Positive: a directly-assembled router served with no health route ──

    #[test]
    fn flags_router_served_without_health() {
        let src = r#"
            async fn main() {
                let app = Router::new().route("/users", get(list));
                axum::serve(listener, app).await.unwrap();
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: a registered health route silences the rule ──────────────

    #[test]
    fn allows_router_with_health_route() {
        let src = r#"
            async fn main() {
                let app = Router::new()
                    .route("/users", get(list))
                    .route("/health", get(health));
                axum::serve(listener, app).await.unwrap();
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_router_with_healthz_alias() {
        let src = r#"
            async fn main() {
                let app = Router::new()
                    .route("/users", get(list))
                    .route("/healthz", get(health));
                axum::serve(listener, app).await.unwrap();
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_router_with_readyz_probe() {
        let src = r#"
            async fn main() {
                let app = Router::new()
                    .route("/users", get(list))
                    .route("/readyz", get(ready));
                axum::serve(listener, app).await.unwrap();
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_router_with_ping_probe() {
        let src = r#"
            async fn main() {
                let app = Router::new()
                    .route("/users", get(list))
                    .route("/ping", get(ping));
                axum::serve(listener, app).await.unwrap();
            }
        "#;
        assert!(run(src).is_empty());
    }

    // ── Negative: routes not visible to this file — absence unprovable ─────

    #[test]
    fn allows_router_built_in_helper() {
        // The router is assembled elsewhere (`build_app()`), so its routes —
        // possibly including `/health` — are out of this file's sight.
        let src = r#"
            async fn main() {
                let app = build_app();
                axum::serve(listener, app).await.unwrap();
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_router_composed_with_merge() {
        // A merged sub-router may itself register the health route.
        let src = r#"
            async fn main() {
                let app = Router::new()
                    .route("/users", get(list))
                    .merge(health_routes());
                axum::serve(listener, app).await.unwrap();
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_router_composed_with_nest() {
        // A nested sub-router may carry a health endpoint under its prefix.
        let src = r#"
            async fn main() {
                let app = Router::new()
                    .route("/users", get(list))
                    .nest("/api", api_routes());
                axum::serve(listener, app).await.unwrap();
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_router_composed_with_nest_service() {
        // `.nest_service` mounts a sub-router/service whose routes are invisible.
        let src = r#"
            async fn main() {
                let app = Router::new()
                    .route("/users", get(list))
                    .nest_service("/api", api_router());
                axum::serve(listener, app).await.unwrap();
            }
        "#;
        assert!(run(src).is_empty());
    }

    // ── Negative: integration-test scaffolding is exempt ───────────────────

    #[test]
    fn allows_served_router_in_tests_dir() {
        // A `tests/` integration test spins up a real server on an ephemeral
        // port; it needs no orchestrator liveness probe (`skip_in_test_dir`).
        let src = r#"
            async fn spawn_app() {
                let app = Router::new().route("/users", get(list));
                axum::serve(listener, app).await.unwrap();
            }
        "#;
        assert!(run_at(src, "tests/api.rs").is_empty());
    }

    // ── Negative: not the real axum entrypoint ─────────────────────────────

    #[test]
    fn allows_bare_serve_call() {
        // A bare `serve(...)` (not the `axum::serve` path) is too generic.
        let src = r#"
            async fn main() {
                let app = Router::new().route("/users", get(list));
                serve(listener, app).await.unwrap();
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_other_module_serve() {
        // `other::serve(...)` is not `axum::serve(...)`.
        let src = r#"
            async fn main() {
                let app = Router::new().route("/users", get(list));
                other::serve(listener, app).await.unwrap();
            }
        "#;
        assert!(run(src).is_empty());
    }
}
