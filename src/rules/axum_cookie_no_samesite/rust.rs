//! axum-cookie-no-samesite backend.
//!
//! Flags a finalized `cookie` builder chain — `Cookie::build((name, value))`
//! terminated by `.build()` or `.finish()` — that never sets the `same_site`
//! attribute. Such a cookie inherits inconsistent SameSite defaults across
//! browsers.
//!
//! Detection walks the receiver chain of a `.build()` / `.finish()`
//! `call_expression` (a `field_expression` function whose field is `build` or
//! `finish`) down to its root. The chain is flagged only when:
//!
//! 1. the root is the `Cookie::build` associated function (bare or
//!    path-qualified, e.g. `cookie::Cookie::build`), and
//! 2. no `.same_site(...)` call appears anywhere in the chain.
//!
//! Any `.same_site(<x>)` call marks the policy as handled and stays silent —
//! the developer set it, whatever the value (`SameSite::Strict`, a config
//! expression, even `SameSite::None`). A chain with no `.same_site(...)` at all
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
    // whether any `.same_site(...)` call sets the policy.
    let mut receiver = func.child_by_field_name("value");
    let mut rooted_at_cookie_build = false;
    let mut same_site_set = false;
    while let Some(recv) = receiver {
        if recv.kind() != "call_expression" { break; }
        let Some(rf) = recv.child_by_field_name("function") else { break; };
        match rf.kind() {
            "field_expression" => {
                if rf
                    .child_by_field_name("field")
                    .and_then(|f| f.utf8_text(source).ok())
                    == Some("same_site")
                {
                    same_site_set = true;
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

    if !rooted_at_cookie_build || same_site_set { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "This `Cookie` is built without `.same_site(...)`, so it inherits \
         inconsistent cross-browser SameSite defaults. Add \
         `.same_site(SameSite::Lax)` (or `SameSite::Strict`) to the builder chain."
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

    // ── Positive: finalized builder chains missing a same_site policy ───────

    #[test]
    fn flags_build_chain_without_same_site() {
        // The exact "should flag" snippet from the issue body.
        let src = r#"fn f() { let c = Cookie::build(("sid", token)).secure(true).http_only(true).build(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_minimal_build_chain() {
        assert_eq!(run(r#"fn f() { let c = Cookie::build(("sid", token)).build(); }"#).len(), 1);
    }

    #[test]
    fn flags_qualified_cookie_build() {
        let src = r#"fn f() { let c = cookie::Cookie::build(("sid", token)).secure(true).build(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_finish_finalizer() {
        let src = r#"fn f() { let c = Cookie::build(("sid", token)).secure(true).finish(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_build_chain_added_to_jar() {
        let src = r#"fn f(jar: CookieJar) { let j = jar.add(Cookie::build(("sid", token)).secure(true).build()); }"#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: policy set / dynamic / non-Cookie / partial chains ────────

    #[test]
    fn allows_same_site_strict() {
        // The exact "should not flag" snippet from the issue body.
        let src = r#"fn f() { let c = Cookie::build(("sid", token)).secure(true).http_only(true).same_site(SameSite::Strict).build(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_same_site_from_variable() {
        // Policy controlled by config/env — the developer set it; not an FP target.
        let src = r#"fn f() { let c = Cookie::build(("sid", token)).same_site(cfg.same_site).build(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_explicit_same_site_none() {
        // Presence of `.same_site(...)` marks the policy as handled, matching the
        // `elysia-cookie-no-samesite` sibling: an explicit `SameSite::None` is a
        // deliberate choice and is not flagged.
        let src = r#"fn f() { let c = Cookie::build(("sid", token)).same_site(SameSite::None).build(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_partial_builder_not_finalized() {
        // A builder handed around without `.build()`/`.finish()` is not flagged;
        // same_site may still be added before finalizing.
        let src = r#"fn f() { let b = Cookie::build(("sid", token)).secure(true); }"#;
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
        // same_site policy is set via a later `set_same_site` statement, out of
        // scope here.
        let src = r#"fn f() { let c = Cookie::new("sid", token); }"#;
        assert!(run(src).is_empty());
    }
}
