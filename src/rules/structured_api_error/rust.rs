//! structured-api-error Rust backend.
//!
//! Flags a bare `panic!` inside an HTTP request handler — an `async fn` in a
//! file that registers routes (Axum/Actix/Rocket). Synchronous route-builder
//! methods and `#[track_caller]` precondition guards are not handlers and are
//! left alone: their panics signal caller programming errors at app build time.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{
    enclosing_fn, fn_is_async, has_outer_attribute, is_in_test_context, is_under_tests_dir,
};
use tree_sitter::Node;

/// Web-framework route-registration attribute macros (`#[get(...)]`, …).
const ROUTE_ATTR_MACROS: [&str; 5] = ["get", "post", "put", "delete", "patch"];

/// True if any node in the AST is a real route registration:
/// a `.route(...)` method call, a `Router::new()` call, or an HTTP-verb
/// attribute macro (`#[get(...)]`, …).
///
/// The scan is AST-based, so routing patterns that appear only in doc-comment
/// examples (doctests) are excluded automatically — comment text is not part of
/// the code tree. Routing constructs inside `#[cfg(test)]` / `#[test]` contexts
/// are skipped too, so a file whose only `.route(...)` lives in its test module
/// is not treated as a route file.
fn is_route_file(root: Node, source: &[u8]) -> bool {
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if is_route_registration(n, source) && !is_in_test_context(n, source) {
            return true;
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if `node` is, on its own, a route-registration construct.
fn is_route_registration(node: Node, source: &[u8]) -> bool {
    match node.kind() {
        "call_expression" => {
            let Some(func) = node.child_by_field_name("function") else {
                return false;
            };
            match func.kind() {
                // `<expr>.route(...)`
                "field_expression" => func
                    .child_by_field_name("field")
                    .and_then(|f| f.utf8_text(source).ok())
                    .is_some_and(|name| name == "route"),
                // `Router::new()` (with or without a leading path).
                "scoped_identifier" => func
                    .utf8_text(source)
                    .is_ok_and(|path| path.ends_with("Router::new")),
                _ => false,
            }
        }
        // `#[get(...)]`, `#[post(...)]`, …
        "attribute_item" => node
            .utf8_text(source)
            .ok()
            .is_some_and(is_route_attr_macro),
        _ => false,
    }
}

/// True if an `attribute_item`'s text names an HTTP-verb route macro, e.g.
/// `#[get("/foo")]` or `#[actix_web::post("/foo")]`. The verb must be a full
/// path segment (preceded by `#[` or `::`) so unrelated attributes whose name
/// merely ends in a verb (`#[my_get(...)]`) are not matched.
fn is_route_attr_macro(attr_text: &str) -> bool {
    ROUTE_ATTR_MACROS.iter().any(|verb| {
        attr_text.contains(&format!("#[{verb}(")) || attr_text.contains(&format!("::{verb}("))
    })
}

/// True if `node` (a `panic!` invocation) sits inside a function shaped like an
/// HTTP request handler, as opposed to route-registration / builder / precondition
/// machinery.
///
/// The rule scopes handler detection to `async fn` deliberately. Request
/// handlers are idiomatically `async`, while the panics that drove false
/// positives — route-builder methods (`route_layer`, `merge`, `set_endpoint`),
/// `const fn` path validators, `#[track_caller]` precondition guards — are
/// synchronous and run at app build time, where `panic!` is the idiomatic
/// signal for a caller programming error. Keying on `async` clears those
/// without per-crate carve-outs. The trade-off is a deliberate false negative:
/// a `panic!` in a synchronous handler (legal but rare) is not flagged. A
/// `panic!` only counts as a handler panic when its enclosing function is:
///
/// - an `async fn`, AND
/// - not annotated `#[track_caller]` (that attribute exists to point a panic at
///   the *caller*, marking the function a precondition guard, never a handler).
///
/// A `panic!` outside any function body (e.g. a module-scope initializer) is not
/// a handler panic either.
fn is_in_request_handler(node: Node, source: &[u8]) -> bool {
    let Some(function) = enclosing_fn(node) else {
        return false;
    };
    fn_is_async(function, source) && !has_outer_attribute(function, source, "track_caller")
}

crate::ast_check! { on ["macro_invocation"] => |node, source, ctx, diagnostics|
    let Some(mac) = node.child_by_field_name("macro") else { return };
    let Ok(mac_name) = mac.utf8_text(source) else { return };

    if is_in_test_context(node, source) || is_under_tests_dir(ctx.path) {
        return;
    }

    if mac_name != "panic" {
        return;
    }

    if !is_in_request_handler(node, source) {
        return;
    }

    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    if !is_route_file(root, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "structured-api-error".into(),
        message: "Bare `panic!` in route handler — use structured error types.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
    fn flags_panic_in_route() {
        let src = "fn setup() { let _ = Router::new(); }\nasync fn handler() { panic!(\"oops\"); }\n";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_panic_outside_route() {
        let src = "fn handler() { panic!(\"oops\"); }\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_panic_in_test_dir() {
        // Router::new() would trigger is_route_file, but path is under tests/
        let src = "use axum::Router;\nfn setup() { let _ = Router::new(); panic!(\"oops\"); }\n";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "tests/helper.rs").is_empty());
    }

    #[test]
    fn ignores_panic_in_cfg_test_module() {
        let src = "#[cfg(test)]\nmod tests {\n    use axum::Router;\n    fn helper() { let _ = Router::new(); panic!(\"oops\"); }\n}\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_panic_in_utility_module() {
        // Reproduces FP from axum/src/response/sse.rs: file imports axum but is not a handler/router
        let src = r#"
use axum::response::sse::Sse;

pub struct Event { flags: u8 }

impl Event {
    pub fn event(mut self) -> Self {
        if self.flags & 1 != 0 {
            panic!("Called Event::event multiple times");
        }
        self
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_panic_when_route_only_in_doctest() {
        // Regression for #1261: axum `sse.rs`. `.route(`/`Router::new()` appear
        // only inside a doc-comment example; the flagged `panic!` is a builder
        // invariant guard, not a route handler.
        let src = r#"
/// SSE event builder.
///
/// ```
/// use axum::{routing::get, Router};
/// let app: Router = Router::new().route("/sse", get(handler));
/// ```
pub struct Event { flags: u8 }

impl Event {
    pub fn event(mut self) -> Self {
        if self.flags & 1 != 0 {
            panic!("Called Event::event multiple times");
        }
        self
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_panic_when_route_registered_in_real_code() {
        // Negative-space guard: real `.route(...)` registration outside any test
        // context still classifies the file, so the handler `panic!` is flagged.
        let src = r#"
use axum::{routing::get, Router};

fn build() -> Router {
    Router::new().route("/sse", get(handler))
}

async fn handler() {
    panic!("boom");
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_panic_with_attribute_macro_route() {
        let src = r#"
#[get("/foo")]
async fn handler() {
    panic!("boom");
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_track_caller_builder_precondition_panic() {
        // Regression for #3245: axum method_routing.rs:887. A `#[track_caller]`
        // builder guard in route-registration machinery — not a handler.
        let src = r#"
use axum::{routing::get, Router};

fn build() -> Router {
    Router::new().route("/x", get(handler))
}

#[track_caller]
fn set_endpoint(out: &mut Endpoint, endpoint: &Endpoint) {
    if out.is_some() {
        panic!(
            "Overlapping method route. Cannot add two method routes that both handle `GET`",
        );
    }
}

async fn handler() {}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_async_builder_method_panic() {
        // Regression for #3245: axum method_routing.rs:1063. A synchronous
        // route-builder method without `#[track_caller]` — still not a handler.
        let src = r#"
use axum::{routing::get, Router};

fn build() -> Router {
    Router::new().route("/x", get(handler))
}

pub fn route_layer<L>(mut self, layer: L) -> Self {
    if self.routes.is_empty() {
        panic!(
            "Adding a route_layer before any routes is a no-op. \
             Add the routes you want the layer to apply to first."
        );
    }
    self
}

async fn handler() {}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_const_fn_path_validator_panic() {
        // Regression for #3245: axum-extra routing/mod.rs:34. A `const fn` path
        // validator runs at compile time and can never be a request handler.
        let src = r#"
use axum::{routing::get, Router};

fn build() -> Router {
    Router::new().route("/x", get(handler))
}

pub const fn validate_static_path(path: &'static str) -> &'static str {
    if path.as_bytes()[0] != b'/' {
        panic!("Paths must start with a `/`. Use \"/\" for root routes")
    }
    path
}

async fn handler() {}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_panic_in_async_handler_returning_impl_into_response() {
        // True positive: a bare `panic!` in a genuine async handler registered
        // via `.route(...)` must still fire.
        let src = r#"
use axum::{response::IntoResponse, routing::get, Router};

fn build() -> Router {
    Router::new().route("/x", get(handler))
}

async fn handler() -> impl IntoResponse {
    panic!("boom");
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
