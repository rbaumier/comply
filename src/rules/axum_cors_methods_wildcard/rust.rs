//! axum-cors-methods-wildcard backend.
//!
//! A `tower_http::cors::CorsLayer` that combines credentials with wildcard
//! methods. Credentialed CORS may not answer with a `*` methods list, and
//! allowing every HTTP verb widens the credentialed attack surface, so two
//! `call_expression` shapes flag:
//!
//! 1. `CorsLayer::very_permissive()` — a `scoped_identifier` constructor whose
//!    final segment is `very_permissive` and whose type segment is `CorsLayer`.
//!    That constructor sets `allow_credentials(true)` together with
//!    request-mirroring methods, so it is the insecure pattern by itself.
//! 2. A builder chain that pairs `.allow_credentials(true)` with a wildcard
//!    `.allow_methods(Any)` / `.allow_methods(AllowMethods::any())`, in either
//!    order. The chain is walked from its outermost `call_expression` down its
//!    receivers; the rule fires once when both method calls are present.
//!
//! An explicit method list — `.allow_credentials(true).allow_methods([Method::GET, Method::POST])`,
//! `.allow_methods(methods)`, a single `Method` — is not `Any` and stays silent,
//! as does `.allow_credentials(false)` and a plain `CorsLayer::permissive()`
//! (which does not enable credentials).

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

/// `CorsLayer::very_permissive()`, including a path-qualified receiver such as
/// `tower_http::cors::CorsLayer::very_permissive()`. Distinct from
/// `permissive()`, which does not enable credentials.
fn is_cors_very_permissive(func: tree_sitter::Node, source: &[u8]) -> bool {
    func.kind() == "scoped_identifier"
        && path_tail(func, source) == "very_permissive"
        && func
            .child_by_field_name("path")
            .is_some_and(|p| path_tail(p, source) == "CorsLayer")
}

/// The `AllowMethods::any` path of an `AllowMethods::any()` call, including a
/// qualified `cors::AllowMethods::any()`.
fn is_allow_methods_any_call(func: tree_sitter::Node, source: &[u8]) -> bool {
    func.kind() == "scoped_identifier"
        && path_tail(func, source) == "any"
        && func
            .child_by_field_name("path")
            .is_some_and(|p| path_tail(p, source) == "AllowMethods")
}

/// The wildcard methods argument: the `Any` unit struct (bare or path-qualified)
/// or an `AllowMethods::any()` call.
fn is_wildcard_methods_arg(arg: tree_sitter::Node, source: &[u8]) -> bool {
    match arg.kind() {
        "identifier" => arg.utf8_text(source).unwrap_or("") == "Any",
        "scoped_identifier" => path_tail(arg, source) == "Any",
        "call_expression" => arg
            .child_by_field_name("function")
            .is_some_and(|f| is_allow_methods_any_call(f, source)),
        _ => false,
    }
}

/// True if any argument of `call` is a wildcard methods value (`Any` / `AllowMethods::any()`).
fn call_has_wildcard_methods(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    args.named_children(&mut cursor)
        .any(|arg| is_wildcard_methods_arg(arg, source))
}

/// True if any argument of `call` is the boolean literal `true`. A runtime
/// value (variable, field, `false`) is not the hardcoded insecure default.
fn call_arg_is_true(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    args.named_children(&mut cursor)
        .any(|arg| arg.kind() == "boolean_literal" && arg.utf8_text(source) == Ok("true"))
}

crate::ast_check! { on ["call_expression"] prefilter = ["allow_credentials", "very_permissive"] => |node, source, ctx, diagnostics|
    // Fire once per method chain: only the outermost call is not itself the
    // receiver (`value`) of an enclosing `field_expression`.
    if node.parent().is_some_and(|p| p.kind() == "field_expression") {
        return;
    }

    let mut has_credentials_true = false;
    let mut has_wildcard_methods = false;
    let mut is_very_permissive = false;

    let mut cur = node;
    while cur.kind() == "call_expression" {
        let Some(func) = cur.child_by_field_name("function") else { break };
        match func.kind() {
            "field_expression" => {
                let method = func
                    .child_by_field_name("field")
                    .and_then(|f| f.utf8_text(source).ok())
                    .unwrap_or("");
                if method == "allow_credentials" && call_arg_is_true(cur, source) {
                    has_credentials_true = true;
                } else if method == "allow_methods" && call_has_wildcard_methods(cur, source) {
                    has_wildcard_methods = true;
                }
                match func.child_by_field_name("value") {
                    Some(receiver) => cur = receiver,
                    None => break,
                }
            }
            "scoped_identifier" => {
                is_very_permissive = is_cors_very_permissive(func, source);
                break;
            }
            _ => break,
        }
    }

    if !(is_very_permissive || (has_credentials_true && has_wildcard_methods)) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "CORS credentials with wildcard methods: pairing `.allow_credentials(true)` with \
         `.allow_methods(Any)` allows every HTTP verb from credentialed requests. Pass an \
         explicit list: `.allow_methods([Method::GET, Method::POST])`."
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

    // ── Positive: credentials paired with wildcard methods ─────────────────

    #[test]
    fn flags_credentials_then_wildcard_methods() {
        let src = "fn app() { let cors = CorsLayer::new().allow_credentials(true).allow_methods(Any); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_methods_then_credentials() {
        // Order-independent: the two method calls may appear in either order.
        let src = "fn app() { let cors = CorsLayer::new().allow_methods(Any).allow_credentials(true); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_credentials_with_allowmethods_any() {
        let src = "fn app() { let cors = CorsLayer::new().allow_credentials(true).allow_methods(AllowMethods::any()); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_mid_chain_pairing() {
        let src = "fn app() { let cors = CorsLayer::new().allow_origin(Any).allow_credentials(true).allow_methods(Any); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_very_permissive() {
        // `very_permissive()` sets credentials + request-mirroring methods by itself.
        assert_eq!(run("fn app() { let cors = CorsLayer::very_permissive(); }").len(), 1);
    }

    #[test]
    fn flags_qualified_very_permissive() {
        assert_eq!(
            run("fn app() { let cors = tower_http::cors::CorsLayer::very_permissive(); }").len(),
            1
        );
    }

    #[test]
    fn fires_once_when_chain_continues_past_the_pairing() {
        // A trailing method after the insecure pairing makes that method the
        // chain root: the walk must still emit exactly one diagnostic, not one
        // per nested `call_expression`.
        let src = "fn app() { let cors = CorsLayer::new().allow_credentials(true).allow_methods(Any).allow_origin(Any); }";
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: restricted / unrelated shapes stay silent ────────────────

    #[test]
    fn allows_credentials_with_method_list() {
        // The issue's canonical safe pattern: an explicit method list.
        let src = "fn app() { let cors = CorsLayer::new().allow_credentials(true).allow_methods([Method::GET, Method::POST]); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_credentials_with_methods_variable() {
        let src = "fn app() { let cors = CorsLayer::new().allow_credentials(true).allow_methods(methods); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_credentials_with_single_method() {
        // `Method::GET` is a `scoped_identifier` whose tail is `GET`, not `Any`.
        let src = "fn app() { let cors = CorsLayer::new().allow_credentials(true).allow_methods(Method::GET); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_wildcard_methods_without_credentials() {
        // `.allow_methods(Any)` alone is not this credentials-specific rule's concern.
        assert!(run("fn app() { let cors = CorsLayer::new().allow_methods(Any); }").is_empty());
    }

    #[test]
    fn allows_credentials_false_with_wildcard() {
        let src = "fn app() { let cors = CorsLayer::new().allow_credentials(false).allow_methods(Any); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_credentials_runtime_bool_with_wildcard() {
        // A runtime-controlled credentials flag is not the hardcoded insecure default.
        let src = "fn app() { let cors = CorsLayer::new().allow_credentials(cfg.creds).allow_methods(Any); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_plain_permissive() {
        // `permissive()` does not enable credentials, so this rule stays silent.
        assert!(run("fn app() { let cors = CorsLayer::permissive(); }").is_empty());
    }

    #[test]
    fn allows_unrelated_very_permissive_constructor() {
        // A `very_permissive` associated fn on another type is not tower_http CORS.
        assert!(run("fn app() { let p = Policy::very_permissive(); }").is_empty());
    }

    #[test]
    fn allows_plain_new() {
        assert!(run("fn app() { let cors = CorsLayer::new(); }").is_empty());
    }
}
