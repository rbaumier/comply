//! axum-jwt-cookie-no-httponly backend.
//!
//! Flags a finalized `cookie` builder chain — `Cookie::build((name, value))`
//! terminated by `.build()` or `.finish()` — whose stored value is a JWT
//! produced by `jsonwebtoken::encode(...)` and that never enables the
//! `http_only` attribute. Such a cookie exposes the token to JavaScript (an XSS
//! vector).
//!
//! This is the JWT-specific sibling of `axum-cookie-no-httponly`: the generic
//! rule flags any http_only-less cookie, this one additionally requires the
//! cookie value to carry a JWT, so it stays meaningful even where the
//! generic rule is relaxed.
//!
//! Detection walks the receiver chain of a `.build()` / `.finish()`
//! `call_expression` down to its root, exactly like `axum-cookie-no-httponly`,
//! and flags only when all hold:
//!
//! 1. the root is the `Cookie::build` associated function (bare or
//!    path-qualified, e.g. `cookie::Cookie::build`), and
//! 2. no `.http_only(<x>)` call in the chain carries a value other than the
//!    boolean literal `false`, and
//! 3. the cookie's value expression is a JWT: it is (or, when the value is a
//!    local binding, its initializer is) a JWT `encode(...)` call — a
//!    path-qualified `jsonwebtoken::encode`, or a bare `encode` that the file
//!    imports from a JWT crate (resolved from the `use` graph, not a file
//!    substring). A non-JWT value (an opaque session id, a `base64::encode`
//!    result, a literal) is left to the generic rule.
//!
//! `.http_only(true)` and `.http_only(<variable/expr>)` mark the attribute as
//! handled and stay silent. A builder never finalized with `.build()`/
//! `.finish()`, and any finalizer rooted at a type other than `Cookie`, are
//! left alone.

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

/// True when a `.http_only(...)` call carries a value other than the boolean
/// literal `false` — `true`, or a variable/config expression. A literal
/// `false` (or a missing argument) does not protect the cookie.
fn http_only_arg_is_non_false(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    let Some(arg) = args.named_children(&mut cursor).next() else {
        return false;
    };
    arg.utf8_text(source).map(str::trim) != Ok("false")
}

/// The value expression — the second element of the `(name, value)` tuple —
/// passed to a `Cookie::build((name, value))` call.
fn cookie_value_expr<'a>(
    build_call: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<tree_sitter::Node<'a>> {
    let args = build_call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let tuple = args.named_children(&mut cursor).next()?;
    if tuple.kind() != "tuple_expression" {
        return None;
    }
    let mut tuple_cursor = tuple.walk();
    let mut elems = tuple.named_children(&mut tuple_cursor);
    elems.next()?; // cookie name
    elems.next() // cookie value
}

/// True when `call` is a JWT `encode(...)` invocation: the function tail is
/// `encode` and it resolves to a JWT crate — either a path-qualified
/// `jsonwebtoken::encode`, or a bare `encode` that the file imports from a JWT
/// crate (`has_jwt_encode_import`, resolved from the `use` graph, not a file
/// substring). Keys the JWT signal on the real API and excludes look-alikes such
/// as `base64::encode`, even when `jsonwebtoken` is used elsewhere in the file.
fn is_jwt_encode_call(call: tree_sitter::Node, source: &[u8], has_jwt_encode_import: bool) -> bool {
    if call.kind() != "call_expression" {
        return false;
    }
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if path_tail(func, source) != "encode" {
        return false;
    }
    match func.kind() {
        "scoped_identifier" => func
            .child_by_field_name("path")
            .and_then(|p| p.utf8_text(source).ok())
            .is_some_and(|path| path.contains("jsonwebtoken")),
        "identifier" => has_jwt_encode_import,
        _ => false,
    }
}

/// Whether `node`'s subtree contains a JWT `encode(...)` call.
fn subtree_has_jwt_encode(
    node: tree_sitter::Node,
    source: &[u8],
    has_jwt_encode_import: bool,
) -> bool {
    if is_jwt_encode_call(node, source, has_jwt_encode_import) {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| subtree_has_jwt_encode(child, source, has_jwt_encode_import))
}

/// Initializer of the `let <name> = <init>` binding in scope at `use_site`: the
/// one declared closest *before* it, within the nearest enclosing
/// block/function/closure. Position-aware so a later shadow of `name` wins over
/// an earlier same-named binding rather than producing a false positive.
fn binding_init<'a>(
    use_site: tree_sitter::Node<'a>,
    name: &str,
    source: &'a [u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut scope = use_site;
    loop {
        scope = scope.parent()?;
        if matches!(
            scope.kind(),
            "block" | "function_item" | "closure_expression"
        ) {
            break;
        }
    }
    let mut best = None;
    collect_preceding_let(scope, name, use_site.start_byte(), source, &mut best);
    best
}

/// Store in `best` the value of the latest-in-source `let <name> = <value>`
/// declared before `use_offset` anywhere in `node`'s subtree.
fn collect_preceding_let<'a>(
    node: tree_sitter::Node<'a>,
    name: &str,
    use_offset: usize,
    source: &'a [u8],
    best: &mut Option<tree_sitter::Node<'a>>,
) {
    if node.kind() == "let_declaration"
        && node.start_byte() < use_offset
        && let Some(text) = node
            .child_by_field_name("pattern")
            .and_then(|p| p.utf8_text(source).ok())
        && text.strip_prefix("mut ").unwrap_or(text) == name
        && let Some(value) = node.child_by_field_name("value")
        && best.is_none_or(|b: tree_sitter::Node| b.start_byte() < value.start_byte())
    {
        *best = Some(value);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_preceding_let(child, name, use_offset, source, best);
    }
}

/// True when the cookie's value carries a JWT: the value expression itself
/// contains a `jsonwebtoken::encode(...)` call, or the value is a local binding
/// whose initializer does.
fn cookie_value_is_jwt(
    value: tree_sitter::Node,
    scope_anchor: tree_sitter::Node,
    source: &[u8],
    has_jwt_encode_import: bool,
) -> bool {
    if subtree_has_jwt_encode(value, source, has_jwt_encode_import) {
        return true;
    }
    if value.kind() == "identifier"
        && let Ok(name) = value.utf8_text(source)
        && let Some(init) = binding_init(scope_anchor, name, source)
    {
        return subtree_has_jwt_encode(init, source, has_jwt_encode_import);
    }
    false
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

    // Walk the receiver chain: capture the root `Cookie::build(...)` call and
    // whether any `.http_only(<non-false>)` call handles the attribute.
    let mut receiver = func.child_by_field_name("value");
    let mut cookie_build_call = None;
    let mut http_only_set = false;
    while let Some(recv) = receiver {
        if recv.kind() != "call_expression" { break; }
        let Some(rf) = recv.child_by_field_name("function") else { break; };
        match rf.kind() {
            "field_expression" => {
                if rf
                    .child_by_field_name("field")
                    .and_then(|f| f.utf8_text(source).ok())
                    == Some("http_only")
                    && http_only_arg_is_non_false(recv, source)
                {
                    http_only_set = true;
                }
                receiver = rf.child_by_field_name("value");
            }
            "scoped_identifier" => {
                if is_cookie_build(rf, source) { cookie_build_call = Some(recv); }
                break;
            }
            _ => break,
        }
    }

    let Some(cookie_build_call) = cookie_build_call else { return };
    if http_only_set { return; }

    // Only fire when the cookie stores a JWT (`jsonwebtoken::encode`). A bare
    // `encode(...)` counts only when `encode` is imported from a JWT crate —
    // resolved from the `use` graph, so a `use base64::encode` alongside an
    // unrelated `jsonwebtoken::decode` in the same file never false-positives.
    let Some(value) = cookie_value_expr(cookie_build_call, source) else { return };
    let has_jwt_encode_import =
        crate::rules::rust_helpers::file_binds_name_to_jwt_crate(node, source, "encode");
    if !cookie_value_is_jwt(value, node, source, has_jwt_encode_import) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "This `Cookie` stores a JWT (`jsonwebtoken::encode`) but is built without \
         `.http_only(true)`, so the token is readable from JavaScript (XSS vector). \
         Add `.http_only(true)` to the builder chain."
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

    // ── Positive: JWT cookie chains missing an http_only flag ──────────────

    #[test]
    fn flags_jwt_cookie_via_let_binding() {
        // The exact "should flag" snippet from the issue body (value bound to
        // `jsonwebtoken::encode`, imported as bare `encode`).
        let src = r#"
use jsonwebtoken::encode;
fn f() {
    let token = encode(&header, &claims, &key)?;
    let c = Cookie::build(("jwt", token)).secure(true).build();
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_jwt_cookie_inline_qualified() {
        // Path-qualified `jsonwebtoken::encode` inline as the cookie value.
        let src = r#"fn f() { let c = Cookie::build(("jwt", jsonwebtoken::encode(&h, &cl, &k).unwrap())).secure(true).build(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_jwt_value_regardless_of_cookie_name() {
        // The signal is the JWT value, not the cookie name — a "session" cookie
        // holding a JWT is flagged too. Also exercises the `.finish()` finalizer.
        let src = r#"
use jsonwebtoken::encode;
fn f() { let token = encode(&h, &c, &k)?; let cookie = Cookie::build(("session", token)).finish(); }
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_qualified_cookie_build_with_jwt() {
        // Path-qualified `cookie::Cookie::build` root still resolves.
        let src = r#"
use jsonwebtoken::encode;
fn f() { let token = encode(&h, &c, &k)?; let c = cookie::Cookie::build(("jwt", token)).build(); }
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_jwt_cookie_explicit_http_only_false() {
        // `.http_only(false)` does not protect the JWT — still flagged.
        let src = r#"
use jsonwebtoken::encode;
fn f() { let token = encode(&h, &c, &k)?; let c = Cookie::build(("jwt", token)).http_only(false).build(); }
"#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: protected / non-JWT / dynamic / partial chains ────────────

    #[test]
    fn allows_jwt_cookie_with_http_only() {
        // The exact "should not flag" snippet from the issue body.
        let src = r#"
use jsonwebtoken::encode;
fn f() { let token = encode(&h, &c, &k)?; let c = Cookie::build(("jwt", token)).secure(true).http_only(true).build(); }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_non_jwt_cookie() {
        // An opaque session id — not a JWT — is left to the generic rule; this
        // JWT-specific rule must stay silent even without `.http_only(true)`.
        let src = r#"fn f() { let sid = generate_session_id(); let c = Cookie::build(("sid", sid)).secure(true).build(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_base64_encode_value() {
        // `base64::encode` shares the `encode` name but is not a JWT; the file
        // does not import `jsonwebtoken`, so the value is not treated as a token.
        let src = r#"
use base64::encode;
fn f() { let data = encode(&raw); let c = Cookie::build(("data", data)).secure(true).build(); }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_shadowed_non_jwt_binding() {
        // The cookie stores the later, non-JWT `token`; the earlier JWT binding
        // is shadowed and out of scope at the cookie — must not flag.
        let src = r#"
use jsonwebtoken::encode;
fn f() {
    let token = encode(&h, &c, &k)?;
    let token = load_opaque_id();
    let c = Cookie::build(("jwt", token)).secure(true).build();
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_base64_encode_beside_jsonwebtoken() {
        // The file verifies bearer tokens with `jsonwebtoken::decode` but the
        // cookie stores a base64 OAuth-state — `encode` resolves to `base64`, not
        // a JWT crate, so the file-wide presence of `jsonwebtoken` must not FP.
        let src = r#"
use jsonwebtoken::{decode, DecodingKey, Validation};
use base64::encode;
fn set_state_cookie(state: &[u8]) {
    let data = encode(state);
    let c = Cookie::build(("oauth_state", data)).secure(true).build();
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_jwt_http_only_from_variable() {
        // http_only controlled by config/env — the developer set it.
        let src = r#"
use jsonwebtoken::encode;
fn f() { let token = encode(&h, &c, &k)?; let c = Cookie::build(("jwt", token)).http_only(is_api).build(); }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_partial_builder_not_finalized() {
        // A builder handed around without `.build()`/`.finish()` is not flagged;
        // http_only may still be added before finalizing.
        let src = r#"
use jsonwebtoken::encode;
fn f() { let token = encode(&h, &c, &k)?; let b = Cookie::build(("jwt", token)).secure(true); }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_cookie_build_finalizer() {
        // A `.build()` on a non-Cookie builder is unrelated, even in a file that
        // uses `jsonwebtoken` and has a `Cookie::build` elsewhere.
        let src = r#"
use jsonwebtoken::encode;
fn f() {
    let token = encode(&h, &c, &k)?;
    let x = RequestBuilder::new().header("a", token).build();
    let _c = Cookie::build(("id", "v")).http_only(true).build();
}
"#;
        assert!(run(src).is_empty());
    }
}
