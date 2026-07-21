//! axum-cookie-no-secure backend.
//!
//! Flags a finalized `cookie` builder chain — `Cookie::build((name, value))`
//! terminated by `.build()` or `.finish()` — that never enables the `secure`
//! attribute. Such a cookie can be transmitted over plain HTTP.
//!
//! Detection walks the receiver chain of a `.build()` / `.finish()`
//! `call_expression` (a `field_expression` function whose field is `build` or
//! `finish`) down to its root. The chain is flagged only when:
//!
//! 1. the root is the `Cookie::build` associated function (bare or
//!    path-qualified, e.g. `cookie::Cookie::build`), and
//! 2. no `.secure(<x>)` call in the chain carries a value other than the
//!    boolean literal `false`.
//!
//! `.secure(true)` and `.secure(<variable/expr>)` (e.g. `.secure(is_prod)`)
//! mark the security as handled and stay silent — the developer controls it.
//! A chain with no `.secure(...)` at all, or with an explicit `.secure(false)`,
//! is flagged. A builder that is never finalized with `.build()`/`.finish()`
//! (a partial builder passed around) and any `.build()`/`.finish()` rooted at a
//! type other than `Cookie` are left alone.

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

/// The `Cookie::build` associated-function path, including a qualified receiver
/// such as `cookie::Cookie::build`.
fn is_cookie_build(func: tree_sitter::Node, source: &[u8]) -> bool {
    func.kind() == "scoped_identifier"
        && path_tail(func, source) == "build"
        && func
            .child_by_field_name("path")
            .is_some_and(|p| path_tail(p, source) == "Cookie")
}

/// True when a `.secure(...)` call carries a value other than the boolean
/// literal `false` — `true`, or a variable/config expression. A literal
/// `false` (or a missing argument) does not secure the cookie.
fn secure_arg_is_non_false(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    let Some(arg) = args.named_children(&mut cursor).next() else {
        return false;
    };
    arg.utf8_text(source).map(str::trim) != Ok("false")
}

crate::ast_check! { on ["call_expression"] prefilter = ["Cookie::build"] => |node, source, ctx, diagnostics|
    // Only the finalizing `.build()` / `.finish()` call closes a builder chain.
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "field_expression" { return; }
    let is_finalizer = func
        .child_by_field_name("field")
        .and_then(|f| f.utf8_text(source).ok())
        .is_some_and(|name| name == "build" || name == "finish");
    if !is_finalizer { return; }

    // Walk the receiver chain: confirm the root is `Cookie::build(...)` and
    // whether any `.secure(<non-false>)` call handles the security attribute.
    let mut receiver = func.child_by_field_name("value");
    let mut rooted_at_cookie_build = false;
    let mut secured = false;
    while let Some(recv) = receiver {
        if recv.kind() != "call_expression" { break; }
        let Some(rf) = recv.child_by_field_name("function") else { break; };
        match rf.kind() {
            "field_expression" => {
                if rf
                    .child_by_field_name("field")
                    .and_then(|f| f.utf8_text(source).ok())
                    == Some("secure")
                    && secure_arg_is_non_false(recv, source)
                {
                    secured = true;
                }
                receiver = rf.child_by_field_name("value");
            }
            "scoped_identifier" => {
                rooted_at_cookie_build = is_cookie_build(rf, source);
                break;
            }
            _ => break,
        }
    }

    if !rooted_at_cookie_build || secured { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "This `Cookie` is built without `.secure(true)`, so it can be sent over \
         plain HTTP. Add `.secure(true)` to the builder chain."
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

    // ── Positive: finalized builder chains missing a secure flag ───────────

    #[test]
    fn flags_build_chain_without_secure() {
        // The exact "should flag" snippet from the issue body.
        let src = r#"fn f() { let c = Cookie::build(("sid", token)).http_only(true).same_site(SameSite::Lax).build(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_minimal_build_chain() {
        assert_eq!(run(r#"fn f() { let c = Cookie::build(("sid", token)).build(); }"#).len(), 1);
    }

    #[test]
    fn flags_explicit_secure_false() {
        let src = r#"fn f() { let c = Cookie::build(("sid", token)).secure(false).build(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_qualified_cookie_build() {
        let src = r#"fn f() { let c = cookie::Cookie::build(("sid", token)).http_only(true).build(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_finish_finalizer() {
        let src = r#"fn f() { let c = Cookie::build(("sid", token)).http_only(true).finish(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_build_chain_added_to_jar() {
        let src = r#"fn f(jar: CookieJar) { let j = jar.add(Cookie::build(("sid", token)).http_only(true).build()); }"#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: secured / dynamic / non-Cookie / partial chains ──────────

    #[test]
    fn allows_secure_true() {
        // The exact "should not flag" snippet from the issue body.
        let src = r#"fn f() { let c = Cookie::build(("sid", token)).secure(true).http_only(true).same_site(SameSite::Lax).build(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_secure_from_variable() {
        // Security controlled by config/env — the developer set it; not an FP target.
        let src = r#"fn f() { let c = Cookie::build(("sid", token)).secure(is_prod).build(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_secure_from_config_expr() {
        let src = r#"fn f() { let c = Cookie::build(("sid", token)).secure(cfg.cookie_secure).build(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_partial_builder_not_finalized() {
        // A builder handed around without `.build()`/`.finish()` is not flagged;
        // secure may still be added before finalizing.
        let src = r#"fn f() { let b = Cookie::build(("sid", token)).http_only(true); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unrelated_build_finalizer() {
        // A `.build()` on a non-Cookie builder is unrelated to cookies.
        let src = r#"fn f() { let x = RequestBuilder::new().header("a", "b").build(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_cookie_new_without_finalizer() {
        // `Cookie::new(...)` returns a `Cookie` directly (no builder chain); the
        // secure flag is set via a later `set_secure` statement, out of scope here.
        let src = r#"fn f() { let c = Cookie::new("sid", token); }"#;
        assert!(run(src).is_empty());
    }
}
