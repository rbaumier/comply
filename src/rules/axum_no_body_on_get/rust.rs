//! axum-no-body-on-get backend.
//!
//! An axum handler receives its input through extractors in its signature, and
//! only the last parameter may consume the request body. Some of those body
//! extractors read the request content-type and reject the request when it does
//! not match: `Json` and `JsonDeserializer` demand `application/json`, and
//! `Multipart` demands `multipart/form-data` with a boundary. HTTP gives a
//! `GET` or `HEAD` body no defined semantics, so a client sends none and the
//! content-type with it — a route that registers such a handler rejects every
//! request before the handler runs.
//!
//! Detection is a single `call_expression` shape, anchored on crate provenance
//! rather than on names:
//!
//! - the callee resolves to `axum::routing::get` / `head` — through a `use`
//!   declaration rooted at `axum`, or written as the qualified path. A chained
//!   `.get(…)` link counts too, but only when the receiver chain bottoms out in
//!   an `axum::routing` constructor, which is what proves the receiver is a
//!   `MethodRouter` and not a map;
//! - the handler argument is a closure, or a plain name that resolves to
//!   exactly one `fn` item of the same file. A handler reached through a module
//!   path lives outside the file's syntax and stays silent;
//! - the handler's last parameter is typed with one of those extractors, and
//!   the file anchors that name to `axum` / `axum_extra`. Only the last
//!   parameter is read: axum's `Handler` impls require every earlier parameter
//!   to implement `FromRequestParts`, so a body extractor in any other position
//!   does not compile.
//!
//! Provenance is read off the name the file writes, so an `as` rename hides the
//! symbol behind it: `use axum::routing::get as g;` stays silent, and the
//! inverse `use axum::routing::post as get;` reports a `POST` route.
//!
//! Body extractors that tolerate an absent body stay silent, because they keep
//! the route answering: `Form` deserializes the query string on `GET`/`HEAD` by
//! design, and `Bytes`, `String` and `Body` yield an empty value. An
//! `Option<Json<…>>` stays silent too — `OptionalFromRequest` answers `None`
//! for a request that carries no content-type.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers;
use tree_sitter::Node;

/// The `axum::routing` constructors whose method carries no request body.
const BODYLESS_ROUTING_FNS: &[&str] = &["get", "head"];

/// Every free function of `axum::routing` that builds a `MethodRouter`. The
/// chain a `.get(…)` link hangs off has to start at one of them for the link to
/// be `MethodRouter::get`. `get` and `head` appear here too: a chain may start
/// at either and take a second bodyless link.
const METHOD_ROUTER_CONSTRUCTORS: &[&str] = &[
    "any",
    "any_service",
    "connect",
    "connect_service",
    "delete",
    "delete_service",
    "get",
    "get_service",
    "head",
    "head_service",
    "on",
    "on_service",
    "options",
    "options_service",
    "patch",
    "patch_service",
    "post",
    "post_service",
    "put",
    "put_service",
    "query",
    "query_service",
    "trace",
    "trace_service",
];

/// The axum extractors whose `FromRequest` impl reads the request content-type
/// and rejects the request when it is absent. Every other body extractor axum
/// ships accepts an empty body, so a bodyless method still reaches the handler.
const CONTENT_TYPE_BOUND_EXTRACTORS: &[&str] = &["Json", "JsonDeserializer", "Multipart"];

/// The crates that own those extractors. `axum_extra` re-exports its own
/// `Multipart` on top of axum's.
const EXTRACTOR_CRATES: &[&str] = &["axum", "axum_extra"];

fn text<'a>(node: Node, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or_default()
}

/// The node naming what a callee ends in: the identifier of `get(…)`, the last
/// segment of `axum::routing::get(…)`, or the method of `post(c).get(…)`.
fn callee_name_node(func: Node) -> Option<Node> {
    match func.kind() {
        "identifier" => Some(func),
        "scoped_identifier" => func.child_by_field_name("name"),
        "field_expression" => func.child_by_field_name("field"),
        _ => None,
    }
}

/// True if the module segments are exactly `expected`.
fn module_is(module: &[String], expected: &[&str]) -> bool {
    module.iter().map(String::as_str).eq(expected.iter().copied())
}

/// True if the path `func` names a free function of `axum::routing`, resolved
/// through the file's `use` graph or written out in full.
fn path_resolves_to_axum_routing(func: Node, source: &[u8]) -> bool {
    let owned = rust_helpers::rust_path_segments(func, source);
    let segments: Vec<&str> = owned.iter().map(String::as_str).collect();
    let Some((name, prefix)) = segments.split_last() else {
        return false;
    };
    match prefix {
        // `use axum::routing::get;` — the file binds the bare name. A path the
        // grammar spells with a prefix it cannot flatten — `<T as R>::get` —
        // arrives here with its prefix dropped, so require a truly bare one.
        [] if func.kind() == "identifier" => {
            rust_helpers::file_binds_name_to_module(func, source, name, |module| {
                module_is(module, &["axum", "routing"])
            })
        }
        // `routing::get(…)` behind a `use axum::routing;`.
        ["routing"] => rust_helpers::file_binds_name_to_module(func, source, "routing", |module| {
            module_is(module, &["axum"])
        }),
        // `axum::routing::get(…)` needs no import at all.
        ["axum", "routing"] => true,
        _ => false,
    }
}

/// True if `expr` evaluates to an axum `MethodRouter`: a chain of method calls
/// whose innermost callee is an `axum::routing` free function. This is what
/// separates `post(create).get(list)` from `cache.get(key)`.
fn is_axum_method_router(expr: Node, source: &[u8]) -> bool {
    let mut current = expr;
    loop {
        if current.kind() != "call_expression" {
            return false;
        }
        let Some(func) = current.child_by_field_name("function") else {
            return false;
        };
        if func.kind() != "field_expression" {
            // A glob `use axum::routing::*;` resolves any unbound name to that
            // module, so the innermost callee also has to name a constructor
            // the module actually exports.
            return callee_name_node(func)
                .is_some_and(|name| METHOD_ROUTER_CONSTRUCTORS.contains(&text(name, source)))
                && path_resolves_to_axum_routing(func, source);
        }
        let Some(receiver) = func.child_by_field_name("value") else {
            return false;
        };
        current = receiver;
    }
}

/// The node naming the route method, when `call` registers its first argument
/// as the handler of an axum `get` / `head` route.
fn bodyless_route_method<'t>(call: Node<'t>, source: &[u8]) -> Option<Node<'t>> {
    let func = call.child_by_field_name("function")?;
    let name_node = callee_name_node(func)?;
    if !BODYLESS_ROUTING_FNS.contains(&text(name_node, source)) {
        return None;
    }
    let anchored = if func.kind() == "field_expression" {
        func.child_by_field_name("value")
            .is_some_and(|receiver| is_axum_method_router(receiver, source))
    } else {
        path_resolves_to_axum_routing(func, source)
    };
    anchored.then_some(name_node)
}

/// The first argument of a call, comments skipped.
fn first_argument<'t>(call: Node<'t>) -> Option<Node<'t>> {
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    args.named_children(&mut cursor).find(|arg| !arg.is_extra())
}

/// The type of the last declared parameter — the only position an axum handler
/// may put a body extractor in.
fn last_parameter_type<'t>(params: Node<'t>) -> Option<Node<'t>> {
    let mut cursor = params.walk();
    let last = params
        .named_children(&mut cursor)
        .filter(|param| !param.is_extra())
        .last()?;
    if last.kind() != "parameter" {
        return None;
    }
    last.child_by_field_name("type")
}

/// Every `fn` item of the file named `name`. A parse-error region is skipped:
/// the items the grammar recovers there are phantoms, and either answer they
/// give — one more candidate, or the only one — is wrong.
fn collect_function_items<'t>(node: Node<'t>, name: &str, source: &[u8], out: &mut Vec<Node<'t>>) {
    if node.is_error() || node.is_missing() {
        return;
    }
    if node.kind() == "function_item"
        && node
            .child_by_field_name("name")
            .is_some_and(|declared| text(declared, source) == name)
    {
        out.push(node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_function_items(child, name, source, out);
    }
}

/// True if `item` is reachable from `site` by its bare name: every module,
/// `impl` block, trait and function that encloses the item encloses the call
/// site too. A `fn` nested in a `mod tests` beside the call site is a different
/// symbol, and so is an inherent method.
fn encloses_site(item: Node, site: Node) -> bool {
    let mut current = item;
    while let Some(parent) = current.parent() {
        if matches!(
            parent.kind(),
            "mod_item" | "impl_item" | "trait_item" | "function_item"
        ) && !(parent.start_byte() <= site.start_byte() && site.end_byte() <= parent.end_byte())
        {
            return false;
        }
        current = parent;
    }
    true
}

/// The single `fn` item this file binds to `name` at `site`. Two items sharing a
/// name sit in different scopes, and picking either would be a guess.
fn unique_function_item<'t>(site: Node<'t>, name: &str, source: &[u8]) -> Option<Node<'t>> {
    let mut found = Vec::new();
    collect_function_items(rust_helpers::root_node(site), name, source, &mut found);
    found.retain(|item| encloses_site(*item, site));
    match found.as_slice() {
        [item] => Some(*item),
        _ => None,
    }
}

/// The type of the last parameter of the handler `call` registers, when that
/// handler is written in this file.
fn handler_last_parameter_type<'t>(call: Node<'t>, source: &[u8]) -> Option<Node<'t>> {
    let handler = first_argument(call)?;
    let params = match handler.kind() {
        "closure_expression" => handler.child_by_field_name("parameters")?,
        "identifier" => {
            unique_function_item(handler, text(handler, source), source)?
                .child_by_field_name("parameters")?
        }
        _ => return None,
    };
    last_parameter_type(params)
}

/// The name of the content-type-bound extractor `ty` denotes, when the file
/// anchors that name to axum.
fn content_type_bound_extractor(ty: Node, source: &[u8]) -> Option<&'static str> {
    let base = match ty.kind() {
        "generic_type" => ty.child_by_field_name("type")?,
        "type_identifier" | "scoped_type_identifier" => ty,
        _ => return None,
    };
    let owned = rust_helpers::rust_path_segments(base, source);
    let segments: Vec<&str> = owned.iter().map(String::as_str).collect();
    let (name, prefix) = segments.split_last()?;
    let extractor = CONTENT_TYPE_BOUND_EXTRACTORS
        .iter()
        .copied()
        .find(|known| known == name)?;
    // Only the crate segment is read: axum exports `Json` from `axum`,
    // `axum::extract` and `axum::response`, all naming the same type.
    let rooted_in_extractor_crate = |module: &[String]| {
        module
            .first()
            .is_some_and(|krate| EXTRACTOR_CRATES.contains(&krate.as_str()))
    };
    let anchored = match prefix {
        // `use axum::Json;` / `use axum::extract::Json;` — the file binds the
        // name. A path whose prefix the grammar cannot flatten also lands here,
        // so require a truly bare one.
        [] if base.kind() == "type_identifier" => {
            rust_helpers::file_binds_name_to_module(ty, source, name, rooted_in_extractor_crate)
        }
        [] => false,
        // `axum::Json<…>` written out, or `extract::Json<…>` behind a
        // `use axum::extract;`.
        [head, ..] => {
            EXTRACTOR_CRATES.contains(head)
                || rust_helpers::file_binds_name_to_module(
                    ty,
                    source,
                    head,
                    rooted_in_extractor_crate,
                )
        }
    };
    anchored.then_some(extractor)
}

crate::ast_check! { on ["call_expression"] prefilter = ["get", "head"] => |node, source, ctx, diagnostics|
    // The prefilter runs per node, so its literals must appear in every call the
    // rule reports. These gates decide nothing — the provenance checks below do.
    // They only stop the file-wide walks those checks run on files that cannot
    // produce a diagnostic. Every lookup is memoized per file.
    if !ctx.source_contains("axum") { return; }
    if !CONTENT_TYPE_BOUND_EXTRACTORS.iter().any(|name| ctx.source_contains(name)) { return; }

    let Some(method) = bodyless_route_method(node, source) else { return };
    let Some(param_type) = handler_last_parameter_type(node, source) else { return };
    let Some(extractor) = content_type_bound_extractor(param_type, source) else { return };

    let method_name = text(method, source);
    // HTTP requires a HEAD response to match its GET twin, so a HEAD route
    // cannot move to a method that carries a body.
    let advice = if method_name == "head" {
        "Read the input from `Query<…>`."
    } else {
        "Read the input from `Query<…>`, or register the handler with `post`."
    };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &method,
        super::META.id,
        format!(
            "`{method_name}(…)` registers a handler that extracts `{extractor}`. That extractor \
             rejects a request whose content-type does not match, so every {upper} request that \
             follows HTTP's no-body convention is rejected before the handler runs. {advice}",
            upper = method_name.to_uppercase(),
        ),
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

    // ── Positive: a bodyless method registers a content-type-bound extractor ──

    #[test]
    fn flags_inline_closure_taking_json() {
        let src = r#"
            use axum::{routing::get, Json, Router};
            fn app() -> Router {
                Router::new().route("/s", get(|Json(b): Json<Query>| async move { b }))
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_named_handler_taking_json() {
        let src = r#"
            use axum::extract::Json;
            use axum::routing::get;
            async fn search(Json(body): Json<Query>) -> String { body.term }
            fn app() -> Router { Router::new().route("/s", get(search)) }
        "#;
        let found = run(src);
        assert_eq!(found.len(), 1);
        // A GET route may move to a method that carries a body, and the message
        // is the only place the rule says so.
        assert!(found[0].message.contains("register the handler with `post`"));
    }

    #[test]
    fn flags_head_route() {
        let src = r#"
            use axum::{routing::head, Json, Router};
            async fn probe(Json(body): Json<Query>) -> String { body.term }
            fn app() -> Router { Router::new().route("/s", head(probe)) }
        "#;
        let found = run(src);
        assert_eq!(found.len(), 1);
        // A HEAD response must match its GET twin, so `post` is not a move the
        // message may propose here.
        assert!(!found[0].message.contains("post"));
    }

    #[test]
    fn flags_fully_qualified_paths_without_imports() {
        let src = r#"
            async fn search(body: axum::Json<Query>) -> String { body.0.term }
            fn app() -> axum::Router {
                axum::Router::new().route("/s", axum::routing::get(search))
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_get_chained_onto_a_method_router() {
        // `post(create)` proves the receiver is a `MethodRouter`, so the
        // chained `.get(list)` is axum's, not a map lookup.
        let src = r#"
            use axum::{routing::post, Json, Router};
            async fn create(Json(new): Json<New>) -> String { new.name }
            async fn list(Json(filter): Json<Filter>) -> String { filter.name }
            fn app() -> Router { Router::new().route("/u", post(create).get(list)) }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multipart_extractor() {
        let src = r#"
            use axum::{extract::Multipart, routing::get, Router};
            async fn upload(parts: Multipart) -> String { String::new() }
            fn app() -> Router { Router::new().route("/u", get(upload)) }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_json_deserializer_extractor() {
        let src = r#"
            use axum::{routing::get, Router};
            use axum_extra::extract::JsonDeserializer;
            async fn search(body: JsonDeserializer<Query>) -> String { String::new() }
            fn app() -> Router { Router::new().route("/s", get(search)) }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_through_a_glob_import_of_axum_routing() {
        let src = r#"
            use axum::routing::*;
            use axum::{Json, Router};
            async fn search(Json(body): Json<Query>) -> String { body.term }
            fn app() -> Router { Router::new().route("/s", get(search)) }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_body_extractor_after_leading_part_extractors() {
        let src = r#"
            use axum::{extract::State, routing::get, Json, Router};
            async fn search(State(db): State<Db>, Json(body): Json<Query>) -> String { body.term }
            fn app() -> Router { Router::new().route("/s", get(search)) }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn reports_the_route_method_position() {
        let src = "use axum::{routing::get, Json, Router};\nfn app() -> Router { Router::new().route(\"/s\", get(|Json(b): Json<Query>| async move { b })) }\n";
        let found = run(src);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 2);
        assert_eq!(found[0].column, 48);
    }

    // ── Negative: extractors that keep the route answering ────────────────
    //
    // Three of these handlers return `Json<Hit>`. That return type only carries
    // the file past the file-wide gate, so that the extractor table itself is
    // what keeps `Path`, `Query`, `Form`, `Bytes` and `String` out. Drop it and
    // the test asserts a silence the gate produces.

    #[test]
    fn allows_query_and_path_extractors() {
        let src = r#"
            use axum::{extract::{Path, Query}, routing::get, Json, Router};
            async fn search(Path(id): Path<u32>, Query(q): Query<Filter>) -> Json<Hit> { Json(hit) }
            fn app() -> Router { Router::new().route("/s/:id", get(search)) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_form_extractor_which_reads_the_query_string_on_get() {
        // axum's `Form` deserializes the query string for GET and HEAD, so a
        // GET route taking `Form<T>` answers normally.
        let src = r#"
            use axum::{routing::get, Form, Json, Router};
            async fn search(Form(filter): Form<Filter>) -> Json<Hit> { Json(hit) }
            fn app() -> Router { Router::new().route("/s", get(search)) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_extractors_that_accept_an_empty_body() {
        // `Bytes`, `String` and `Body` yield an empty value on a bodyless
        // request instead of rejecting it.
        let src = r#"
            use axum::{body::Bytes, routing::get, Json, Router};
            async fn raw(payload: Bytes) -> Json<Hit> { Json(hit) }
            async fn text(payload: String) -> String { payload }
            fn app() -> Router {
                Router::new().route("/r", get(raw)).route("/t", get(text))
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_an_optional_body_extractor() {
        // `Option<Json<T>>` goes through `OptionalFromRequest`, which yields
        // `None` when the request carries no content-type.
        let src = r#"
            use axum::{routing::get, Json, Router};
            async fn search(body: Option<Json<Query>>) -> String { String::new() }
            fn app() -> Router { Router::new().route("/s", get(search)) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_json_on_a_method_that_carries_a_body() {
        let src = r#"
            use axum::{routing::post, Json, Router};
            async fn create(Json(new): Json<New>) -> String { new.name }
            fn app() -> Router { Router::new().route("/u", post(create)) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_json_as_a_response_type() {
        let src = r#"
            use axum::{extract::Query, routing::get, Json, Router};
            async fn search(Query(q): Query<Filter>) -> Json<Vec<Hit>> { Json(vec![]) }
            fn app() -> Router { Router::new().route("/s", get(search)) }
        "#;
        assert!(run(src).is_empty());
    }

    // ── Negative: the anchors that keep the rule off unrelated code ───────

    #[test]
    fn allows_a_json_type_from_another_crate() {
        // `sqlx::types::Json` is not an axum extractor, so a handler holding
        // one is not registering a body extractor.
        let src = r#"
            use axum::routing::get;
            use sqlx::types::Json;
            async fn search(cached: Json<Query>) -> String { cached.0.term }
            fn app() -> Router { Router::new().route("/s", get(search)) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_get_function_from_another_crate() {
        let src = r#"
            use axum::Json;
            use crate::router::get;
            async fn search(Json(body): Json<Query>) -> String { body.term }
            fn app() -> Router { Router::new().route("/s", get(search)) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_map_lookup_named_get() {
        // The receiver is a field, not an `axum::routing` call, so this `get`
        // is a map lookup even though its argument is a body-taking handler.
        let src = r#"
            use axum::{routing::post, Json, Router};
            async fn create(Json(new): Json<New>) -> String { new.name }
            fn lookup(cache: &Cache) -> Option<&Handler> { cache.handlers.get(create) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_handler_reached_through_a_module_path() {
        // `handlers::search` is another module's symbol. The same-file
        // `fn search` is a decoy: resolving the path by its last segment would
        // read the wrong signature.
        let src = r#"
            use axum::{routing::get, Json, Router};
            async fn search(Json(body): Json<Query>) -> String { body.term }
            fn app() -> Router { Router::new().route("/s", get(handlers::search)) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_handler_shadowed_by_a_same_named_test_fixture() {
        // The route's handler is imported; the `fn search` of `mod tests` is a
        // different symbol and its signature says nothing about the route.
        let src = r#"
            use axum::{routing::get, Router};
            use crate::handlers::search;
            fn app() -> Router { Router::new().route("/s", get(search)) }

            #[cfg(test)]
            mod tests {
                use axum::Json;
                async fn search(Json(body): Json<Query>) -> String { body.term }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_get_imported_from_another_crate_beside_a_glob_import_of_axum_routing() {
        // A glob only fills the names the file does not bind itself, so the
        // explicit `use` decides: this `get` belongs to the other crate.
        let src = r#"
            use axum::routing::*;
            use axum::Json;
            use crate::router::get;
            async fn search(Json(body): Json<Query>) -> String { body.term }
            fn app() -> Router { Router::new().route("/s", get(search)) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_map_lookup_beside_a_glob_import_of_axum_routing() {
        // `registry` is not an `axum::routing` constructor, so the glob must
        // not turn `registry().get(create)` into a `MethodRouter` link.
        let src = r#"
            use axum::routing::*;
            use axum::Json;
            async fn create(Json(new): Json<New>) -> String { new.name }
            fn lookup() -> Option<&'static Handler> { registry().get(create) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_qualified_trait_path_named_get() {
        // `<Registry as Lookup>::get` is not a bare name, so it must not
        // resolve through the file's `use axum::routing::get`.
        let src = r#"
            use axum::{routing::get, Json, Router};
            async fn create(Json(new): Json<New>) -> String { new.name }
            fn lookup() -> Option<&'static Handler> { <Registry as Lookup>::get(create) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_qualified_trait_path_named_json() {
        // `<Db as Store>::Json` is not a bare type name, so it must not resolve
        // through the file's `use axum::Json`.
        let src = r#"
            use axum::{routing::get, Json, Router};
            async fn search(cached: <Db as Store>::Json) -> String { String::new() }
            fn app() -> Router { Router::new().route("/s", get(search)) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_chain_rooted_in_another_crate() {
        // `post` comes from another crate, so the chain is not a `MethodRouter`
        // even though its innermost callee carries a constructor's name.
        let src = r#"
            use axum::Json;
            use crate::routing::post;
            async fn create(Json(new): Json<New>) -> String { new.name }
            fn build() { post(create).get(create); }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_routing_import_renamed_by_an_alias() {
        // Provenance is read off the name the file writes, so the rule cannot
        // see through `as`. It stays silent rather than guess.
        let src = r#"
            use axum::{routing::get as route_get, Json, Router};
            async fn search(Json(body): Json<Query>) -> String { body.term }
            fn app() -> Router { Router::new().route("/s", route_get(search)) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_two_in_scope_handlers_of_the_same_name() {
        // Both `fn search` items reach the call site — the file-level one has no
        // scoping ancestor, and `fn app` spans the route — and both take a body
        // extractor, so any pick would report. The rule refuses to pick.
        let src = r#"
            use axum::{routing::get, Json, Router};
            async fn search(Json(body): Json<Query>) -> String { body.term }
            fn app() -> Router {
                async fn search(Json(body): Json<Query>) -> String { body.term }
                Router::new().route("/s", get(search))
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_handler_defined_in_a_sibling_module() {
        // Neither `mod` spans the call site, so neither `fn search` is the
        // symbol the route names.
        let src = r#"
            use axum::{routing::get, Json, Router};
            mod v1 { pub async fn search(Json(b): Json<Query>) -> String { b.term } }
            mod v2 { pub async fn search(Query(q): Query<Filter>) -> String { q.term } }
            fn app() -> Router { Router::new().route("/s", get(search)) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_handler_without_parameters() {
        let src = r#"
            use axum::{routing::get, Json, Router};
            async fn health() -> Json<Status> { Json(Status::Ok) }
            fn app() -> Router { Router::new().route("/health", get(health)) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_json_parameter_that_is_not_last() {
        // axum requires every parameter but the last to implement
        // `FromRequestParts`, so this signature is not a handler at all.
        let src = r#"
            use axum::{extract::Query, routing::get, Json, Router};
            async fn search(Json(body): Json<Query>, Query(q): Query<Filter>) -> String { q.term }
            fn app() -> Router { Router::new().route("/s", get(search)) }
        "#;
        assert!(run(src).is_empty());
    }
}
