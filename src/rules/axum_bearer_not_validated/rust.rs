//! axum-bearer-not-validated backend.
//!
//! Flags an axum handler that extracts a bearer credential but then ignores it:
//! the token is accepted without ever being read, so every request — including
//! one carrying a forged or empty token — is let through.
//!
//! Detection requires all of:
//!
//! 1. a `function_item` with a parameter whose declared type contains
//!    `Authorization<Bearer>` (whitespace-insensitive). That generic type is the
//!    `axum-extra` / `headers` typed-header credential for the `Bearer` scheme —
//!    it only ever appears through `TypedHeader<Authorization<Bearer>>`, the sole
//!    extractor that produces a bearer token. `Authorization<Basic>` and every
//!    other extractor are not matched.
//! 2. at least one named binding in that parameter's pattern (e.g. `auth` in
//!    `TypedHeader(auth)`, `bearer` in `TypedHeader(Authorization(bearer))`).
//!    A pattern that binds nothing (`TypedHeader(_)`) is left alone.
//! 3. **none** of those bindings is referenced anywhere in the function body.
//!
//! The third condition is the security signal: reading the credential — at the
//! minimum `auth.token()`, or passing `auth` to a verify/lookup call — references
//! the binding, so any handler that validates the token references it and stays
//! silent. Only a handler that extracts the bearer and never touches the binding
//! fires. Because *any* use of the binding suppresses the diagnostic, a handler
//! that validates the token by a means other than `.token()` (for instance
//! `lookup(&auth)`) is never a false positive.
//!
//! Raw `authorization`-header reads (`HeaderMap::get("authorization")`) are
//! deliberately out of scope: a `HeaderMap` parameter carries no bearer-specific
//! signal, so keying on it would fire on unrelated header handling. The rule
//! keys only on the unambiguous `Authorization<Bearer>` extractor type.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["function_item"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // The flagged shape always names the `Bearer` credential in a parameter
        // type, so a file without the substring can never fire.
        Some(&["Bearer"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        let Some(params) = node.child_by_field_name("parameters") else {
            return;
        };
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };

        let mut cursor = params.walk();
        for param in params.named_children(&mut cursor) {
            if is_ignored_bearer_extractor(param, body, source) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &param,
                    super::META.id,
                    "A `Bearer` token is extracted via `TypedHeader<Authorization<Bearer>>` but \
                     the extracted credential is never read — the handler accepts any token. Read \
                     `auth.token()`, validate it, and return `401` when it is invalid."
                        .to_string(),
                    Severity::Error,
                ));
                return;
            }
        }
    }
}

/// True when `param` extracts a bearer credential (`Authorization<Bearer>`) whose
/// binding is never referenced in the handler `body` — the handler accepts any
/// token. A pattern that binds nothing (`TypedHeader(_)`) and any parameter whose
/// binding is read (validated) both return `false`.
fn is_ignored_bearer_extractor(
    param: tree_sitter::Node,
    body: tree_sitter::Node,
    source: &[u8],
) -> bool {
    if param.kind() != "parameter" {
        return false;
    }
    let Some(ty) = param.child_by_field_name("type") else {
        return false;
    };
    if !type_is_bearer_authorization(ty, source) {
        return false;
    }
    let Some(pattern) = param.child_by_field_name("pattern") else {
        return false;
    };
    let mut bindings = Vec::new();
    collect_bindings(pattern, source, &mut bindings);
    if bindings.is_empty() {
        return false;
    }
    // Fire only when none of the extracted bindings is read in the body.
    !bindings.iter().any(|b| identifier_used_in(body, b, source))
}

/// True when a parameter's type mentions `Authorization<Bearer>` (comparing with
/// all whitespace removed, so `Authorization < Bearer >` and the usual
/// `TypedHeader<Authorization<Bearer>>` wrapper both match). That generic type is
/// the bearer-scheme typed header; `Authorization<Basic>` and other extractors
/// do not contain the substring and are not matched.
fn type_is_bearer_authorization(ty: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = ty.utf8_text(source) else {
        return false;
    };
    let normalized: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    normalized.contains("Authorization<Bearer>")
}

/// Collect the binding identifiers introduced by a parameter pattern.
///
/// A tuple-struct pattern's constructor path (`TypedHeader` in
/// `TypedHeader(auth)`, `Authorization` in `TypedHeader(Authorization(bearer))`)
/// is the pattern's `type` field and names a type, not a binding, so it is
/// skipped; the sub-patterns inside the parentheses are recursed into. A leaf
/// `identifier` is a binding; a `_` wildcard introduces none.
fn collect_bindings(node: tree_sitter::Node, source: &[u8], out: &mut Vec<String>) {
    if node.kind() == "identifier" {
        if let Ok(text) = node.utf8_text(source) {
            out.push(text.to_string());
        }
        return;
    }
    let type_field_id = node.child_by_field_name("type").map(|n| n.id());
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if Some(child.id()) == type_field_id {
            continue;
        }
        collect_bindings(child, source, out);
    }
}

/// True when `name` appears as an `identifier` anywhere within `node` (the
/// function body). A reference such as `auth.token()`, `verify(auth.token())`, or
/// `lookup(&auth)` all surface `auth` as an `identifier`, so any use of the
/// extracted binding is detected.
fn identifier_used_in(node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    if node.kind() == "identifier" && node.utf8_text(source).ok() == Some(name) {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| identifier_used_in(child, name, source))
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

    // ── Positive: the bearer is extracted but never read ────────────────────

    #[test]
    fn flags_bearer_extracted_but_unused() {
        // The "should flag" snippet from the issue body: `auth` is never touched.
        let src = r#"async fn h(TypedHeader(auth): TypedHeader<Authorization<Bearer>>) -> impl IntoResponse { ok() }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_nested_destructure_unused() {
        // `TypedHeader(Authorization(bearer))` binds `bearer`, which is unused.
        let src = r#"async fn h(TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>) -> impl IntoResponse { ok() }"#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: the extracted credential is read / validated ───────────────

    #[test]
    fn allows_token_validated() {
        // The "should not flag" case: the token is passed to a verify call.
        let src = r#"async fn h(TypedHeader(auth): TypedHeader<Authorization<Bearer>>) -> impl IntoResponse {
    if verify(auth.token()) { ok() } else { unauthorized() }
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_binding_passed_by_reference() {
        // Validation that never calls `.token()` — passes the whole extractor to a
        // lookup — still references `auth`, so it must not false-positive.
        let src = r#"async fn h(TypedHeader(auth): TypedHeader<Authorization<Bearer>>) -> impl IntoResponse {
    lookup_session(&auth)
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_nested_destructure_validated() {
        let src = r#"async fn h(TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>) -> impl IntoResponse {
    validate(bearer.token())
}"#;
        assert!(run(src).is_empty());
    }

    // ── Negative: not a bearer extractor ─────────────────────────────────────

    #[test]
    fn allows_basic_auth_extractor() {
        // `Authorization<Basic>` is a different credential scheme, not a bearer token.
        let src = r#"async fn h(TypedHeader(auth): TypedHeader<Authorization<Basic>>) -> impl IntoResponse { ok() }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unrelated_extractor() {
        let src = r#"async fn h(State(db): State<Db>) -> impl IntoResponse { ok() }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_wildcard_binding() {
        // `TypedHeader(_)` binds no identifier to inspect; left alone (under-fire).
        let src = r#"async fn h(TypedHeader(_): TypedHeader<Authorization<Bearer>>) -> impl IntoResponse { ok() }"#;
        assert!(run(src).is_empty());
    }
}
