//! structured-api-error Rust backend.
//!
//! Flags a bare `panic!` inside an HTTP request handler. A function is a handler
//! when the file registers it as one — an HTTP-verb route attribute macro
//! (`#[get("/x")]`) or a `.route(...)` call naming it — or when it is an
//! `async fn` in a file that registers routes. Route-builder methods,
//! `#[track_caller]` precondition guards and synchronous helpers no registration
//! names are not handlers: their panics report a caller programming error at app
//! build time, which is what `panic!` is for.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::CheckCtx;
use crate::rules::rust_helpers::{
    enclosing_fn, fn_is_async, has_declared_attribute_segment, has_outer_attribute, is_test_code,
    root_node,
};
use std::collections::HashSet;
use tree_sitter::Node;

/// Web-framework route-registration attribute macros (`#[get(...)]`, …), matched
/// on the attribute path's last segment so `#[actix_web::get("/x")]` counts too.
const ROUTE_ATTR_MACROS: [&str; 5] = ["get", "post", "put", "delete", "patch"];

/// Node kinds a route registration can wrap its handler in: the method-router
/// call chain (`get(handler)`, `web::get().to(handler)`). The handler scan
/// descends through these only, so it stops at an inline handler's body and a
/// call made *inside* a handler registers nothing.
const HANDLER_CHAIN_KINDS: [&str; 3] = ["arguments", "call_expression", "field_expression"];

/// True if any node in the AST is a real route registration: a `.route(...)`
/// method call or a `Router::new()` call.
///
/// The scan is AST-based, so routing patterns that appear only in doc-comment
/// examples (doctests) are excluded automatically — comment text is not part of
/// the code tree. Routing constructs inside `#[cfg(test)]` / `#[test]` contexts
/// are skipped too, so a file whose only `.route(...)` lives in its test module
/// is not treated as a route file.
fn is_route_file(root: Node, source: &[u8], ctx: &CheckCtx) -> bool {
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if is_route_registration(n, source) && !is_test_code(n, source, ctx) {
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
    if is_route_method_call(node, source) {
        return true;
    }
    // `Router::new()` (with or without a leading path).
    node.kind() == "call_expression"
        && node
            .child_by_field_name("function")
            .filter(|func| func.kind() == "scoped_identifier")
            .and_then(|func| func.utf8_text(source).ok())
            .is_some_and(|path| path.ends_with("Router::new"))
}

/// True if `node` is a `<expr>.route(...)` call — the method Axum and Actix both
/// expose to bind a path to its handler.
fn is_route_method_call(node: Node, source: &[u8]) -> bool {
    node.kind() == "call_expression"
        && node
            .child_by_field_name("function")
            .filter(|func| func.kind() == "field_expression")
            .and_then(|func| func.child_by_field_name("field"))
            .and_then(|field| field.utf8_text(source).ok())
            .is_some_and(|name| name == "route")
}

/// The handler names this file registers: every identifier written as an
/// argument of a `.route(<path>, …)` call, including the ones nested in the
/// framework's method-router chain (`get(handler)`, `web::get().to(handler)`).
/// A path-qualified argument contributes its last segment, so
/// `get(handlers::list)` registers `list`.
///
/// Registrations in `#[cfg(test)]` / `#[test]` contexts are skipped, on the same
/// ground as [`is_route_file`]: a test wiring up a router says nothing about
/// which production functions serve requests.
fn registered_handler_names<'a>(root: Node, source: &'a [u8], ctx: &CheckCtx) -> HashSet<&'a str> {
    let mut names = HashSet::new();
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if is_route_method_call(node, source)
            && !is_test_code(node, source, ctx)
            && let Some(arguments) = node.child_by_field_name("arguments")
        {
            collect_handler_names(arguments, source, &mut names);
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    names
}

/// Adds to `names` every function name `arguments` — a route registration's
/// argument list — names, following the method-router chain
/// ([`HANDLER_CHAIN_KINDS`]) down to each nested argument list.
fn collect_handler_names<'a>(arguments: Node, source: &'a [u8], names: &mut HashSet<&'a str>) {
    let mut cursor = arguments.walk();
    let mut stack = vec![arguments];
    while let Some(node) = stack.pop() {
        if node.kind() == "arguments" {
            for child in node.named_children(&mut cursor) {
                if let Some(name) = argument_function_name(child, source) {
                    names.insert(name);
                }
            }
        }
        for child in node.named_children(&mut cursor) {
            if HANDLER_CHAIN_KINDS.contains(&child.kind()) {
                stack.push(child);
            }
        }
    }
}

/// The bare function name an argument names: `handler` for `handler`, `list` for
/// `handlers::list`. Any other argument shape (a literal path, a closure, a call)
/// names no function.
fn argument_function_name<'a>(node: Node, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "identifier" => node.utf8_text(source).ok(),
        "scoped_identifier" => node.child_by_field_name("name")?.utf8_text(source).ok(),
        _ => None,
    }
}

/// True if `function` is an HTTP request handler, as opposed to
/// route-registration / builder / precondition machinery.
///
/// Two signals answer yes, and both are proof the function serves requests:
///
/// - the file registers it — an HTTP-verb route attribute macro on the function
///   itself, or a `.route(...)` call naming it. This holds for a synchronous
///   handler, which Axum and Actix both accept.
/// - it is an `async fn` in a [route file](is_route_file). Request handlers are
///   idiomatically `async`, and this reaches the ones a *different* file
///   registers.
///
/// `#[track_caller]` answers no whatever else holds: the attribute exists to
/// point a panic at the *caller*, which marks a precondition guard.
///
/// A route module's own machinery — builder methods (`route_layer`, `merge`,
/// `set_endpoint`), `const fn` path validators, synchronous helpers — is
/// synchronous and unregistered, so no signal reaches it.
fn is_request_handler(function: Node, source: &[u8], ctx: &CheckCtx) -> bool {
    if has_outer_attribute(function, source, "track_caller") {
        return false;
    }
    if has_declared_attribute_segment(function, source, &ROUTE_ATTR_MACROS) {
        return true;
    }
    let root = root_node(function);
    if !is_route_file(root, source, ctx) {
        return false;
    }
    fn_is_async(function, source) || is_named_in_route_registration(function, source, ctx, root)
}

/// True if a `.route(...)` call in this file names `function`.
fn is_named_in_route_registration(
    function: Node,
    source: &[u8],
    ctx: &CheckCtx,
    root: Node,
) -> bool {
    let Some(name) = function
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(source).ok())
    else {
        return false;
    };
    registered_handler_names(root, source, ctx).contains(name)
}

crate::ast_check! { on ["macro_invocation"] => |node, source, ctx, diagnostics|
    let Some(mac) = node.child_by_field_name("macro") else { return };
    let Ok(mac_name) = mac.utf8_text(source) else { return };

    if mac_name != "panic" {
        return;
    }

    if is_test_code(node, source, ctx) {
        return;
    }

    let Some(function) = enclosing_fn(node) else { return };
    if !is_request_handler(function, source, ctx) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "structured-api-error".into(),
        message: "Bare `panic!` in route handler — use structured error types.".into(),
        severity: Severity::Error,
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
    fn flags_panic_in_sync_handler_registered_by_name() {
        // Regression for #3817: axum supports synchronous handlers. `handler` is
        // named in the `.route(...)` registration, so its bare `panic!` fires even
        // though the function is not `async`.
        let src = r#"
use axum::{response::IntoResponse, routing::get, Router};

fn build() -> Router {
    Router::new().route("/x", get(handler))
}

fn handler() -> impl IntoResponse {
    panic!("boom")
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_panic_in_sync_handler_with_route_attribute_macro() {
        // Regression for #3817: an actix handler declared with `#[get("/foo")]` is
        // registered by the attribute macro, so a synchronous one is a handler too.
        let src = r#"
#[get("/foo")]
fn handler() -> String {
    panic!("boom")
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_panic_in_unregistered_sync_helper() {
        // Negative space for #3817: a synchronous helper in a route file that no
        // registration names is not a handler — widening to sync handlers must not
        // flag every `panic!` a route module makes.
        let src = r#"
use axum::{routing::get, Router};

fn build() -> Router {
    Router::new().route("/x", get(handler))
}

fn parse_port(raw: &str) -> u16 {
    panic!("port must be a number")
}

async fn handler() {}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn ignores_panic_in_sync_fn_named_only_by_a_test_router() {
        // A router wired up inside `#[cfg(test)]` says nothing about which
        // production functions serve requests, so it registers no handler.
        let src = r#"
use axum::{routing::get, Router};

fn build() -> Router {
    Router::new().route("/a", get(list))
}

async fn list() {}

fn parse_port(raw: &str) -> u16 {
    panic!("port must be a number")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire() -> Router {
        Router::new().route("/b", get(parse_port))
    }
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
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
