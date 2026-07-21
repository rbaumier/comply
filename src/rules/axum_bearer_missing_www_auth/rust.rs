//! axum-bearer-missing-www-auth backend.
//!
//! Flags a `401 Unauthorized` response returned as an axum `IntoResponse`
//! 2-tuple `(StatusCode::UNAUTHORIZED, body).into_response()` when the file never
//! sets a `WWW-Authenticate` header. RFC 7235 (and RFC 6750 for bearer auth)
//! requires a 401 to carry a `WWW-Authenticate` challenge naming the accepted
//! scheme; a 401 without it tells the client nothing about how to authenticate.
//!
//! Detection requires all of:
//!
//! 1. a `tuple_expression` with exactly two elements whose first element is a
//!    path ending in `StatusCode::UNAUTHORIZED` (bare `StatusCode::UNAUTHORIZED`
//!    or a qualified form such as `axum::http::StatusCode::UNAUTHORIZED`).
//!    Whitespace is stripped before comparison. The leading `::` in the qualified
//!    check keeps the segment boundary exact, so `MyStatusCode::UNAUTHORIZED` is
//!    not matched.
//! 2. the second element is an obvious response *body* — a string literal, a
//!    `Json(...)` / `Html(...)` wrapper, a `.to_string()` call, or a `format!` /
//!    `formatdoc!` macro. axum implements `IntoResponse` for `(StatusCode, T)`
//!    where `T` can itself be headers (`HeaderMap`,
//!    `[(HeaderName, HeaderValue); N]`), so a second element that could carry the
//!    challenge — a bare identifier, a plain call such as `bearer_challenge()`,
//!    or an array literal — is left alone. Only a body with no header slot is a
//!    genuine missing-challenge 401.
//! 3. that tuple is the receiver of a `.into_response()` call — the axum
//!    `IntoResponse` conversion. This is the response-position gate: it grounds
//!    the tuple as an actual axum response (not a `(StatusCode, T)` pair in a
//!    lookup table, a test vector, or non-axum `http`-crate client code, none of
//!    which call `.into_response()`).
//! 4. the file sets no `WWW-Authenticate` header anywhere. Any occurrence of the
//!    typed constant `WWW_AUTHENTICATE` or the `WWW-Authenticate` /
//!    `www-authenticate` string literal exempts the whole file — this covers a
//!    challenge attached after conversion (`res.headers_mut().insert(...)`).
//!
//! The header-bearing safe forms stay silent structurally: a three-element
//! response tuple and a two-element `(StatusCode, <headers>)` tuple are both
//! skipped (the former by the two-element requirement, the latter by the body
//! gate), and any file mentioning the header is skipped by the file guard. A bare
//! `StatusCode::UNAUTHORIZED` or a `Response::builder().status(…)` chain is left
//! alone too (the rule leans to under-fire): absence of the header cannot be
//! proven structurally there, so keying on the unambiguous
//! `(StatusCode::UNAUTHORIZED, body).into_response()` shape keeps false positives
//! at zero.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["tuple_expression"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Every flagged shape names `StatusCode::UNAUTHORIZED`, so a file without
        // the `UNAUTHORIZED` substring can never fire.
        Some(&["UNAUTHORIZED"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        // Only the two-element `(StatusCode::UNAUTHORIZED, body)` response tuple.
        // A three-element tuple carries a header list (`(status, [headers], body)`)
        // that may set the challenge, so it is left alone.
        if node.named_child_count() != 2 {
            return;
        }
        let Some(first) = node.named_child(0) else {
            return;
        };
        if !is_status_unauthorized(first, source) {
            return;
        }
        // Body gate: the second element must be an obvious response *body*.
        // axum implements `IntoResponse` for `(StatusCode, T)` where `T` can
        // itself be headers (`HeaderMap`, `[(HeaderName, HeaderValue); N]`), so a
        // second element that could be a header carrier — a bare identifier, a
        // plain call (`bearer_challenge()`), or an array literal — is left alone
        // (it may set the challenge). Only a plain-text / JSON / HTML / formatted
        // body, which has no header slot, is a genuine missing-challenge 401.
        let Some(second) = node.named_child(1) else {
            return;
        };
        if !is_response_body(second, source) {
            return;
        }
        // Response-position gate: the tuple must be `.into_response()`'d. This
        // grounds it as an axum response and excludes lookup tables, test
        // vectors, and non-axum `http`-crate tuples.
        if !is_into_response_receiver(node, source) {
            return;
        }
        // A file that sets the `WWW-Authenticate` challenge anywhere is exempt —
        // the header may be attached after conversion or on another response.
        if file_sets_www_authenticate(ctx) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "A `401 Unauthorized` response is built without a `WWW-Authenticate` header \
             (RFC 7235 / RFC 6750). Attach a challenge, e.g. \
             `(StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, \"Bearer\")], body)`."
                .to_string(),
            Severity::Error,
        ));
    }
}

/// True when `node`'s text (whitespace stripped) is a path naming
/// `StatusCode::UNAUTHORIZED`, whether bare or qualified
/// (`axum::http::StatusCode::UNAUTHORIZED`). The leading `::` in the qualified
/// check keeps the segment boundary exact, so `MyStatusCode::UNAUTHORIZED` is
/// not matched.
fn is_status_unauthorized(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else {
        return false;
    };
    let normalized: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    normalized == "StatusCode::UNAUTHORIZED"
        || normalized.ends_with("::StatusCode::UNAUTHORIZED")
}

/// True when `node` is an obvious axum response *body* — a plain-text, JSON,
/// HTML, or formatted-string payload. A `(StatusCode, body)` tuple with such a
/// second element has no header slot, so a missing `WWW-Authenticate` challenge
/// is genuine.
///
/// Everything ambiguous is treated as NOT a body, so the rule under-fires: a
/// bare identifier or a plain call may return a `HeaderMap`, and an array literal
/// may be `[(HeaderName, HeaderValue); N]`. axum implements `IntoResponse` for
/// `(StatusCode, T)` where `T` can itself be headers, so those shapes may set the
/// challenge and must not be flagged.
fn is_response_body(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "string_literal" | "raw_string_literal" => true,
        "call_expression" => is_body_call(node, source),
        "macro_invocation" => is_format_macro(node, source),
        _ => false,
    }
}

/// True when a `call_expression` produces a response body: a `Json(...)` /
/// `Html(...)` wrapper (bare or path-qualified), or a `…​.to_string()` call.
fn is_body_call(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    match func.kind() {
        "identifier" | "scoped_identifier" => {
            let Ok(text) = func.utf8_text(source) else {
                return false;
            };
            let last = text.rsplit("::").next().unwrap_or(text).trim();
            matches!(last, "Json" | "Html")
        }
        "field_expression" => {
            func.child_by_field_name("field")
                .and_then(|f| f.utf8_text(source).ok())
                == Some("to_string")
        }
        _ => false,
    }
}

/// True when a `macro_invocation` is a string-building macro (`format!` /
/// `formatdoc!`) — a formatted response body.
fn is_format_macro(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(macro_node) = node.child_by_field_name("macro") else {
        return false;
    };
    matches!(
        macro_node.utf8_text(source).ok(),
        Some("format") | Some("formatdoc")
    )
}

/// True when `tuple` is the receiver of a `.into_response()` method call, i.e.
/// `(tuple).into_response()`. tree-sitter-rust models this as a `call_expression`
/// whose `function` is a `field_expression` with `value == tuple` and a `field`
/// named `into_response`.
fn is_into_response_receiver(tuple: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(field_expr) = tuple.parent() else {
        return false;
    };
    if field_expr.kind() != "field_expression"
        || field_expr.child_by_field_name("value") != Some(tuple)
    {
        return false;
    }
    let Some(field) = field_expr.child_by_field_name("field") else {
        return false;
    };
    if field.utf8_text(source).ok() != Some("into_response") {
        return false;
    }
    // The `.into_response` access must actually be called.
    field_expr.parent().map(|call| call.kind()) == Some("call_expression")
}

/// True when the file mentions the `WWW-Authenticate` header in any of the forms
/// axum code uses to set it: the typed `WWW_AUTHENTICATE` constant, or the
/// canonical / lowercased string literal. All three are memoized substring
/// checks, so this stays cheap when called per node.
fn file_sets_www_authenticate(ctx: &CheckCtx) -> bool {
    ctx.source_contains("WWW_AUTHENTICATE")
        || ctx.source_contains("WWW-Authenticate")
        || ctx.source_contains("www-authenticate")
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

    // ── Positive: a 401 response tuple with no WWW-Authenticate in the file ──

    #[test]
    fn flags_401_tuple_without_www_authenticate() {
        // The "should flag" snippet from the issue body.
        let src = r#"async fn h() -> impl IntoResponse {
    (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_qualified_status_path() {
        // A fully-qualified `StatusCode` path is the same violation.
        let src = r#"async fn h() -> impl IntoResponse {
    (axum::http::StatusCode::UNAUTHORIZED, "no").into_response()
}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_json_body_401() {
        // A `Json(err)` body is a genuine missing-challenge 401.
        let src = r#"async fn h() -> impl IntoResponse {
    (StatusCode::UNAUTHORIZED, Json(err)).into_response()
}"#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: the second element may carry the WWW-Authenticate challenge ─

    #[test]
    fn allows_headers_from_helper_call() {
        // `(StatusCode, HeaderMap)` is a valid axum response: the helper may
        // return a `HeaderMap` that sets the challenge, and the file does not
        // mention the header token, so the file guard cannot see it. The body
        // gate keeps it silent structurally.
        let src = r#"async fn h() -> impl IntoResponse {
    (StatusCode::UNAUTHORIZED, bearer_challenge()).into_response()
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_headers_from_identifier() {
        // A bare identifier could be a `HeaderMap` binding — left alone.
        let src = r#"async fn h() -> impl IntoResponse {
    (StatusCode::UNAUTHORIZED, headers).into_response()
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_header_array_second_element() {
        // `(StatusCode, [(HeaderName, HeaderValue); N])` is a valid header-setting
        // response; the array literal is not a body, so it is left alone even
        // when the file guard cannot see an aliased header constant.
        let src = r#"async fn h() -> impl IntoResponse {
    (StatusCode::UNAUTHORIZED, [(WWW_AUTH_ALIAS, hv)]).into_response()
}"#;
        assert!(run(src).is_empty());
    }

    // ── Negative: the file sets a WWW-Authenticate challenge ─────────────────

    #[test]
    fn allows_401_with_header_constant() {
        // The "should not flag" case: the challenge is attached via the typed
        // `WWW_AUTHENTICATE` constant in a 3-tuple (skipped structurally by the
        // two-element requirement, and by the file guard).
        let src = r#"async fn h() -> impl IntoResponse {
    (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, "Bearer")], "unauthorized").into_response()
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_401_with_header_string_literal() {
        let src = r#"async fn h() -> Response {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header("WWW-Authenticate", "Bearer realm=\"api\"")
        .body("no".into())
        .unwrap()
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_401_with_header_set_after_conversion() {
        // The challenge is attached after `.into_response()`; the file mentions
        // the header, so the file guard exempts it.
        let src = r#"async fn h() -> impl IntoResponse {
    let mut res = (StatusCode::UNAUTHORIZED, "no").into_response();
    res.headers_mut().insert("www-authenticate", "Bearer".parse().unwrap());
    res
}"#;
        assert!(run(src).is_empty());
    }

    // ── Negative: not an axum response tuple ─────────────────────────────────

    #[test]
    fn allows_status_message_const_table() {
        // A `(StatusCode, &str)` lookup table is data, not a response — it is
        // never `.into_response()`'d, so it must stay silent.
        let src = r#"const STATUS_MESSAGES: &[(StatusCode, &str)] = &[
    (StatusCode::UNAUTHORIZED, "AUTH_REQUIRED"),
    (StatusCode::FORBIDDEN, "FORBIDDEN"),
];"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_tuple_in_vec_not_response() {
        // Table-driven test / classification vectors build `(StatusCode, T)`
        // pairs that are never converted to a response.
        let src = r#"fn cases() {
    let v = vec![(StatusCode::UNAUTHORIZED, "/admin"), (StatusCode::OK, "/public")];
    let _ = v;
}"#;
        assert!(run(src).is_empty());
    }

    // ── Negative: not a 401 response tuple ───────────────────────────────────

    #[test]
    fn allows_non_401_status() {
        let src = r#"async fn h() -> impl IntoResponse {
    (StatusCode::OK, "ok").into_response()
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_status_not_first_element() {
        let src = r#"async fn h() -> impl IntoResponse {
    (headers, StatusCode::UNAUTHORIZED).into_response()
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_similarly_named_status_type() {
        // `MyStatusCode::UNAUTHORIZED` is not axum's `StatusCode`.
        let src = r#"async fn h() -> impl IntoResponse {
    (MyStatusCode::UNAUTHORIZED, "no").into_response()
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_status_not_a_tuple() {
        // A bare `StatusCode::UNAUTHORIZED` return is deliberately out of scope
        // (the rule keys only on the tuple shape; leans to under-fire).
        let src = r#"async fn h() -> StatusCode {
    StatusCode::UNAUTHORIZED
}"#;
        assert!(run(src).is_empty());
    }
}
