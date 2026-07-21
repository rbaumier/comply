//! axum-cors-wildcard backend.
//!
//! Two `tower_http::cors::CorsLayer` shapes flag this rule, both modelled as a
//! `call_expression`:
//!
//! 1. `CorsLayer::permissive()` / `CorsLayer::very_permissive()` — a
//!    `scoped_identifier` function whose final segment is `permissive` or
//!    `very_permissive` and whose owning type segment is `CorsLayer`. Both
//!    constructors reflect every request's `Origin` back, so any site can reach
//!    the API.
//! 2. `<builder>.allow_origin(Any)` / `.allow_origin(AllowOrigin::any())` — a
//!    `field_expression` function whose field is `allow_origin` and whose sole
//!    argument is the wildcard `Any` unit struct (bare or path-qualified) or
//!    `AllowOrigin::any()`. `Any`/`AllowOrigin::any()` come from
//!    `tower_http::cors`, so the method name plus that argument is unambiguously
//!    the permissive CORS idiom.
//!
//! A restricted origin — `.allow_origin("https://app.example.com".parse::<HeaderValue>().unwrap())`,
//! a `HeaderValue`, or an explicit origin list — is not `Any` and stays silent.

use crate::diagnostic::{Diagnostic, Severity};

/// Final segment of a path node: the `name` of a `scoped_identifier`, or the
/// whole text of a plain `identifier`.
fn path_tail<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> &'a str {
    match node.kind() {
        "scoped_identifier" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or(""),
        _ => node.utf8_text(source).unwrap_or(""),
    }
}

/// `CorsLayer::permissive()` / `CorsLayer::very_permissive()`, including a
/// path-qualified receiver such as `tower_http::cors::CorsLayer::permissive()`.
fn is_permissive_constructor(scoped: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(name) = scoped.child_by_field_name("name") else {
        return false;
    };
    if !matches!(name.utf8_text(source).unwrap_or(""), "permissive" | "very_permissive") {
        return false;
    }
    scoped
        .child_by_field_name("path")
        .is_some_and(|path| path_tail(path, source) == "CorsLayer")
}

/// The wildcard origin argument: the `Any` unit struct (bare or path-qualified)
/// or an `AllowOrigin::any()` call.
fn is_wildcard_origin_arg(arg: tree_sitter::Node, source: &[u8]) -> bool {
    match arg.kind() {
        "identifier" => arg.utf8_text(source).unwrap_or("") == "Any",
        "scoped_identifier" => path_tail(arg, source) == "Any",
        "call_expression" => arg
            .child_by_field_name("function")
            .is_some_and(|f| is_allow_origin_any_call(f, source)),
        _ => false,
    }
}

/// The `AllowOrigin::any` path of an `AllowOrigin::any()` call, including a
/// qualified `cors::AllowOrigin::any()`.
fn is_allow_origin_any_call(func: tree_sitter::Node, source: &[u8]) -> bool {
    func.kind() == "scoped_identifier"
        && path_tail(func, source) == "any"
        && func
            .child_by_field_name("path")
            .is_some_and(|p| path_tail(p, source) == "AllowOrigin")
}

/// `<builder>.allow_origin(<wildcard>)`. `call` is the outer `call_expression`
/// and `field` its `field_expression` function.
fn is_wildcard_allow_origin(
    field: tree_sitter::Node,
    call: tree_sitter::Node,
    source: &[u8],
) -> bool {
    let is_allow_origin = field
        .child_by_field_name("field")
        .is_some_and(|f| f.utf8_text(source).unwrap_or("") == "allow_origin");
    if !is_allow_origin {
        return false;
    }
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    args.named_children(&mut cursor)
        .any(|arg| is_wildcard_origin_arg(arg, source))
}

crate::ast_check! { on ["call_expression"] prefilter = ["permissive", "allow_origin"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    let flagged = match func.kind() {
        "scoped_identifier" => is_permissive_constructor(func, source),
        "field_expression" => is_wildcard_allow_origin(func, node, source),
        _ => false,
    };
    if !flagged { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Permissive CORS: this `CorsLayer` reflects any origin. Restrict it with \
         `.allow_origin(\"https://your-domain.com\".parse::<HeaderValue>().unwrap())`."
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

    // ── Positive: the permissive constructors and wildcard origin ──────────

    #[test]
    fn flags_cors_permissive() {
        assert_eq!(run("fn app() { let cors = CorsLayer::permissive(); }").len(), 1);
    }

    #[test]
    fn flags_cors_very_permissive() {
        assert_eq!(run("fn app() { let cors = CorsLayer::very_permissive(); }").len(), 1);
    }

    #[test]
    fn flags_qualified_permissive() {
        assert_eq!(
            run("fn app() { let cors = tower_http::cors::CorsLayer::permissive(); }").len(),
            1
        );
    }

    #[test]
    fn flags_allow_origin_any() {
        assert_eq!(run("fn app() { let cors = CorsLayer::new().allow_origin(Any); }").len(), 1);
    }

    #[test]
    fn flags_allow_origin_alloworigin_any() {
        assert_eq!(
            run("fn app() { let cors = CorsLayer::new().allow_origin(AllowOrigin::any()); }").len(),
            1
        );
    }

    #[test]
    fn flags_allow_origin_any_mid_chain() {
        let src = "fn app() { let cors = CorsLayer::new().allow_methods([Method::GET]).allow_origin(Any); }";
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: restricted / unrelated shapes stay silent ────────────────

    #[test]
    fn allows_restricted_origin() {
        let src = "fn app() { let cors = CorsLayer::new().allow_origin(\"https://app.example.com\".parse::<HeaderValue>().unwrap()); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_origin_from_variable() {
        let src = "fn app() { let cors = CorsLayer::new().allow_origin(allowed_origin); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_origin_list() {
        let src = "fn app() { let cors = CorsLayer::new().allow_origin([origin_a, origin_b]); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_alloworigin_restricted_constructors() {
        // The closest safe sibling to the flagged `AllowOrigin::any()`: the
        // restricted `AllowOrigin::exact` / `AllowOrigin::list` constructors
        // are not `any` and must stay silent.
        let exact = "fn app() { let cors = CorsLayer::new().allow_origin(AllowOrigin::exact(origin)); }";
        let list = "fn app() { let cors = CorsLayer::new().allow_origin(AllowOrigin::list([a, b])); }";
        assert!(run(exact).is_empty());
        assert!(run(list).is_empty());
    }

    #[test]
    fn allows_plain_new() {
        assert!(run("fn app() { let cors = CorsLayer::new(); }").is_empty());
    }

    #[test]
    fn allows_unrelated_permissive_constructor() {
        // A `permissive` associated fn on some other type is not tower_http CORS.
        assert!(run("fn app() { let p = Policy::permissive(); }").is_empty());
    }

    #[test]
    fn allows_any_arg_on_unrelated_method() {
        // `Any` passed to a method other than `allow_origin` is not a CORS wildcard.
        assert!(run("fn app() { let x = builder.match_type(Any); }").is_empty());
    }
}
