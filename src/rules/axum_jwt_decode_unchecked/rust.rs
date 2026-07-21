//! axum-jwt-decode-unchecked backend.
//!
//! Flags a `jsonwebtoken` `decode`/`decode_header` call whose `Result` is
//! consumed directly by `.unwrap()` / `.expect(...)`. `jsonwebtoken::decode`
//! returns `Err` for a token whose signature, `exp`, or claims fail validation,
//! so unwrapping it turns an invalid or expired token into a panic — a
//! request-triggered denial of service — instead of a `401`.
//!
//! Detection requires all of:
//!
//! 1. an outer `call_expression` whose function is a `field_expression` whose
//!    `field` is `unwrap` or `expect`, and
//! 2. whose receiver is itself a `call_expression` that is a `jsonwebtoken`
//!    free-function call to `decode` / `decode_header`.
//!
//! A call counts as `jsonwebtoken`'s only through one of these grounded shapes,
//! so a different crate's `decode` never collides:
//!
//! - fully qualified `jsonwebtoken::decode` / `jsonwebtoken::decode_header`
//!   (the path names the crate); any other qualifier (`base64::decode`,
//!   `hex::decode`, …) is rejected;
//! - a bare `decode_header(token)` — a name unique to `jsonwebtoken` — with
//!   exactly one argument;
//! - a bare `decode::<T>(token, &key, &validation)` — the generic three-argument
//!   signature of `jsonwebtoken::decode`. A bare `decode` without a turbofish or
//!   with a different arity is left alone (that shape is `base64`/`hex`/`serde`
//!   `decode`, not the JWT API).
//!
//! The two BARE shapes fire only when the file also references the
//! `jsonwebtoken` crate (a bare `decode`/`decode_header` reaches the JWT API
//! solely through `use jsonwebtoken::{decode, decode_header}`, so the token
//! `jsonwebtoken` is always present). Without that reference the bare shape is
//! indistinguishable from a user's own generic `fn decode<T>(a, b, c)` or
//! another crate's `decode_header`, so it is left alone. The fully-qualified
//! shape needs no such gate — its path already names the crate.
//!
//! Method-call receivers (`engine.decode(x)`, the `base64` `Engine::decode`
//! form) are never matched — `jsonwebtoken::decode` is a free function, so the
//! callee is only ever an `identifier` / `scoped_identifier`, never a
//! `field_expression`. Consuming the `Result` any other way (`?`, `match`,
//! `.map_err`, …) is not an unchecked unwrap and is left alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["call_expression"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Both flagged shapes contain the substring `decode`.
        Some(&["decode"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        // Outer node must be `<receiver>.unwrap()` / `<receiver>.expect(...)`.
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        if function.kind() != "field_expression" {
            return;
        }
        let Some(consumer) = function
            .child_by_field_name("field")
            .and_then(|f| f.utf8_text(source).ok())
        else {
            return;
        };
        if consumer != "unwrap" && consumer != "expect" {
            return;
        }
        // The receiver must itself be the `jsonwebtoken` decode call.
        let Some(receiver) = function.child_by_field_name("value") else {
            return;
        };
        if receiver.kind() != "call_expression" {
            return;
        }
        // A bare `decode`/`decode_header` is only trusted when the file references
        // the `jsonwebtoken` crate; the fully-qualified shape carries the crate
        // name itself and ignores this flag.
        let file_references_jsonwebtoken = ctx.source.contains("jsonwebtoken");
        let Some(decode_fn) =
            jsonwebtoken_decode_call(receiver, source, file_references_jsonwebtoken)
        else {
            return;
        };

        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!(
                "`jsonwebtoken::{decode_fn}(...)` result is consumed with `.{consumer}()` — an \
                 invalid or expired token panics instead of being rejected. Propagate the `Result` \
                 with `?` or `match` on the `Err` arm and return `401`."
            ),
            Severity::Error,
        ));
    }
}

/// If `call` is a `jsonwebtoken` free-function call to `decode` / `decode_header`
/// — recognised only through the grounded shapes documented at the module top —
/// return the function's name (`"decode"` / `"decode_header"`). `None` for any
/// other call, so a different crate's `decode` never matches.
///
/// `file_references_jsonwebtoken` gates the two BARE shapes only: a bare
/// `decode`/`decode_header` reaches the JWT API solely via `use jsonwebtoken::…`,
/// so the crate token is always present when the call is genuinely `jsonwebtoken`.
/// The fully-qualified shape is definitive by its path and ignores this flag.
fn jsonwebtoken_decode_call<'a>(
    call: tree_sitter::Node,
    source: &'a [u8],
    file_references_jsonwebtoken: bool,
) -> Option<&'a str> {
    let func = call.child_by_field_name("function")?;
    // Peel a turbofish: `decode::<Claims>` is a `generic_function` wrapping the callee.
    let (callee, has_turbofish) = match func.kind() {
        "generic_function" => (func.child_by_field_name("function")?, true),
        _ => (func, false),
    };
    let (qualifier, name) = match callee.kind() {
        "identifier" => (None, callee.utf8_text(source).ok()?),
        "scoped_identifier" => {
            let name = callee.child_by_field_name("name")?.utf8_text(source).ok()?;
            let qualifier = callee
                .child_by_field_name("path")
                .and_then(|p| p.utf8_text(source).ok());
            (qualifier, name)
        }
        // A `field_expression` callee is a method call (`engine.decode(x)`) —
        // `jsonwebtoken::decode` is a free function, so this is never it.
        _ => return None,
    };
    if name != "decode" && name != "decode_header" {
        return None;
    }
    let arg_count = call
        .child_by_field_name("arguments")
        .map(|args| {
            let mut cursor = args.walk();
            args.named_children(&mut cursor).count()
        })
        .unwrap_or(0);

    match qualifier {
        // Fully qualified `jsonwebtoken::decode` / `…::decode_header`: the path
        // names the crate, so it is definitively the JWT API.
        Some(path) if qualifier_is_jsonwebtoken(path) => Some(name),
        // Any other qualifier (`base64::decode`, `hex::decode`, …) is a different
        // crate's `decode`.
        Some(_) => None,
        // Bare call (`use jsonwebtoken::{decode, decode_header};`). Ground on the
        // real signatures AND the file referencing the crate, so `hex::decode(x)`,
        // `base64::decode(x)`, or a user's own generic `decode` never collide.
        None if file_references_jsonwebtoken => match name {
            "decode_header" if arg_count == 1 => Some(name),
            "decode" if has_turbofish && arg_count == 3 => Some(name),
            _ => None,
        },
        None => None,
    }
}

/// True when a `scoped_identifier`'s path resolves to the `jsonwebtoken` crate —
/// its final segment is `jsonwebtoken` (so `jsonwebtoken::decode` and a
/// re-qualified `crate::jsonwebtoken::decode` both count, but `base64` / `hex`
/// do not).
fn qualifier_is_jsonwebtoken(path: &str) -> bool {
    path.rsplit("::").next().map(str::trim) == Some("jsonwebtoken")
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

    // ── Positive: the decode Result is unwrapped ────────────────────────────

    #[test]
    fn flags_decode_unwrap() {
        // The "should flag" snippet from the issue body, in a file that imports
        // the crate (a bare `decode` reaches the JWT API only through this `use`).
        let src = r#"use jsonwebtoken::decode;
fn f() { let data = decode::<Claims>(token, &key, &validation).unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_decode_expect() {
        let src = r#"use jsonwebtoken::decode;
fn f() { let data = decode::<Claims>(token, &key, &validation).expect("bad token"); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_decode_header_unwrap() {
        let src = r#"use jsonwebtoken::decode_header;
fn f() { let header = decode_header(token).unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_fully_qualified_decode_unwrap() {
        // Fully qualified: definitive by path, no separate import needed.
        let src =
            r#"fn f() { let data = jsonwebtoken::decode::<Claims>(token, &key, &validation).unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_fully_qualified_decode_header_unwrap() {
        // A qualified `decode_header` is definitive by path, regardless of arity.
        let src = r#"fn f() { let header = jsonwebtoken::decode_header(token).unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: the Result is handled, not unwrapped ──────────────────────

    #[test]
    fn allows_decode_question_mark() {
        // The "should not flag" snippet from the issue body.
        let src = r#"use jsonwebtoken::decode;
fn f() -> Result<(), E> { let data = decode::<Claims>(token, &key, &validation)?; Ok(()) }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_decode_match() {
        let src = r#"use jsonwebtoken::decode;
fn f() { match decode::<Claims>(token, &key, &validation) { Ok(d) => use_it(d), Err(_) => reject() } }"#;
        assert!(run(src).is_empty());
    }

    // ── Negative: other crates' `decode` is not the JWT API ─────────────────

    #[test]
    fn allows_base64_decode_unwrap() {
        let src = r#"fn f() { let bytes = base64::decode(s).unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_hex_decode_unwrap() {
        let src = r#"fn f() { let bytes = hex::decode(s).unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_engine_decode_method_unwrap() {
        // `Engine::decode` is a method call, never the `jsonwebtoken` free fn.
        let src = r#"fn f() { let bytes = STANDARD.decode(s).unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_decode_single_arg_unwrap() {
        // A bare one-arg `decode(x)` is a `base64`/`hex`-style decode, not the JWT API.
        let src = r#"use jsonwebtoken::decode;
fn f() { let bytes = decode(s).unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_decode_no_turbofish_with_import() {
        // Import present, but the type-inferred (no-turbofish) form is not the
        // grounded three-argument turbofish signature — an accepted false negative,
        // kept narrow so the shape gate still bites even when the crate is imported.
        let src = r#"use jsonwebtoken::decode;
fn f() { let data = decode(token, key, validation).unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    // ── Negative: the bare shape without the `jsonwebtoken` reference ────────

    #[test]
    fn allows_bare_generic_decode_without_jsonwebtoken() {
        // A user's own generic three-arg `decode::<T>(a, b, c)` in a file with no
        // JWT involvement is structurally identical to the JWT call but must not fire.
        let src = r#"fn decode<T>(_a: A, _b: B, _c: C) -> Result<T, E> { unimplemented!() }
fn f() { let data = decode::<Foo>(a, b, c).unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_decode_header_without_jsonwebtoken() {
        // A bare `decode_header(x)` with no `jsonwebtoken` reference could be any
        // crate's free function; it is left alone.
        let src = r#"fn f() { let h = decode_header(x).unwrap(); }"#;
        assert!(run(src).is_empty());
    }
}
