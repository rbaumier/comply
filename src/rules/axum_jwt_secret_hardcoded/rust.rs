//! axum-jwt-secret-hardcoded backend.
//!
//! Flags `EncodingKey::from_secret(<literal>)` / `DecodingKey::from_secret(<literal>)`
//! — the `jsonwebtoken` associated functions that derive an HMAC signing /
//! verification key from raw secret bytes — when the argument is a hardcoded
//! string or byte-string literal. A secret baked into the source leaks through
//! version control.
//!
//! Detection requires all of:
//!
//! 1. a `call_expression` whose function is the `scoped_identifier`
//!    `EncodingKey::from_secret` or `DecodingKey::from_secret` — bare, or
//!    path-qualified (`jsonwebtoken::EncodingKey::from_secret`); a method call
//!    `x.from_secret(...)` (a `field_expression` function) and any other type's
//!    `from_secret` are not the jsonwebtoken API and never match, and
//! 2. the first argument reduces to a hardcoded literal: a `string_literal` /
//!    `raw_string_literal` (covering `b"…"`, `"…"`, `r"…"`, `br"…"`), directly
//!    or through a byte-view call (`.as_ref()` / `.as_bytes()` / `.as_slice()`),
//!    e.g. `"secret".as_bytes()`.
//!
//! Any argument that is a variable, a field access, `std::env::var(...)`, or any
//! other runtime expression is not a literal and is left alone — the rule fires
//! only on a secret embedded in the source.

use crate::diagnostic::{Diagnostic, Severity};

/// Final path segment: the `name` of a `scoped_identifier`, or the whole text
/// of a plain `identifier`.
fn path_tail<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> &'a str {
    match node.kind() {
        "scoped_identifier" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or(""),
        _ => node.utf8_text(source).unwrap_or(""),
    }
}

/// True when `func` is `EncodingKey::from_secret` or `DecodingKey::from_secret`,
/// including a path-qualified receiver such as
/// `jsonwebtoken::EncodingKey::from_secret`.
fn is_key_from_secret(func: tree_sitter::Node, source: &[u8]) -> bool {
    if func.kind() != "scoped_identifier" || path_tail(func, source) != "from_secret" {
        return false;
    }
    func.child_by_field_name("path")
        .is_some_and(|p| matches!(path_tail(p, source), "EncodingKey" | "DecodingKey"))
}

/// True when `expr` reduces to a hardcoded string/byte-string literal — the
/// literal itself, or a literal read as bytes through a `.as_ref()` /
/// `.as_bytes()` / `.as_slice()` call (the idiomatic ways to hand a literal to
/// `from_secret(&[u8])`). A variable, a field access, `std::env::var(...)`, or
/// any other call is not a literal and returns false.
fn is_hardcoded_literal(expr: tree_sitter::Node, source: &[u8]) -> bool {
    match expr.kind() {
        "string_literal" | "raw_string_literal" => true,
        "call_expression" => {
            let Some(func) = expr.child_by_field_name("function") else {
                return false;
            };
            if func.kind() != "field_expression" {
                return false;
            }
            let is_byte_view = func
                .child_by_field_name("field")
                .and_then(|f| f.utf8_text(source).ok())
                .is_some_and(|m| matches!(m, "as_ref" | "as_bytes" | "as_slice"));
            is_byte_view
                && func
                    .child_by_field_name("value")
                    .is_some_and(|recv| is_hardcoded_literal(recv, source))
        }
        _ => false,
    }
}

crate::ast_check! { on ["call_expression"] prefilter = ["from_secret"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if !is_key_from_secret(func, source) { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(arg) = args.named_children(&mut cursor).next() else { return };
    if !is_hardcoded_literal(arg, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "JWT secret passed to `from_secret` is a hardcoded literal — it leaks via source \
         control. Read it from `std::env::var(...)` or a secret manager instead."
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

    // ── Positive: hardcoded literal secrets ─────────────────────────────────

    #[test]
    fn flags_encoding_key_byte_string_literal() {
        // The exact "should flag" snippet from the issue body.
        let src = r#"fn f() { let k = EncodingKey::from_secret(b"super-secret"); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_decoding_key_byte_string_literal() {
        let src = r#"fn f() { let k = DecodingKey::from_secret(b"super-secret"); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_string_literal_as_ref() {
        let src = r#"fn f() { let k = EncodingKey::from_secret("secret".as_ref()); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_string_literal_as_bytes() {
        let src = r#"fn f() { let k = EncodingKey::from_secret("secret".as_bytes()); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_byte_string_literal_as_slice() {
        let src = r#"fn f() { let k = EncodingKey::from_secret(b"secret".as_slice()); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_path_qualified_encoding_key() {
        let src = r#"fn f() { let k = jsonwebtoken::EncodingKey::from_secret(b"super-secret"); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_raw_byte_string_literal() {
        let src = r#"fn f() { let k = EncodingKey::from_secret(br"super-secret"); }"#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: secret resolved at runtime ────────────────────────────────

    #[test]
    fn allows_env_var_secret() {
        // The exact "should not flag" snippet from the issue body.
        let src = r#"fn f() -> Result<(), E> { let k = EncodingKey::from_secret(std::env::var("JWT_SECRET")?.as_bytes()); Ok(()) }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_variable_secret() {
        let src = r#"fn f(secret: &[u8]) { let k = EncodingKey::from_secret(secret); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_variable_ref_secret() {
        let src = r#"fn f(secret: Vec<u8>) { let k = EncodingKey::from_secret(&secret); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_variable_as_bytes() {
        let src = r#"fn f(secret: String) { let k = EncodingKey::from_secret(secret.as_bytes()); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_config_field_secret() {
        let src = r#"fn f(cfg: &Config) { let k = EncodingKey::from_secret(cfg.jwt_secret.as_bytes()); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_named_constant_secret() {
        // A reference to a named constant is not an inline literal; leave it to
        // `no-hardcoded-secret`. Precision over recall.
        let src = r#"const SECRET: &[u8] = b"x"; fn f() { let k = EncodingKey::from_secret(SECRET); }"#;
        assert!(run(src).is_empty());
    }

    // ── Negative: not the jsonwebtoken API ──────────────────────────────────

    #[test]
    fn allows_from_secret_on_other_type() {
        // A `from_secret` associated fn on an unrelated type is not the
        // jsonwebtoken key API.
        let src = r#"fn f() { let k = MyCipher::from_secret(b"super-secret"); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_from_secret_method_call() {
        // A method call `builder.from_secret(...)` is not the associated fn.
        let src = r#"fn f(builder: KeyBuilder) { let k = builder.from_secret(b"super-secret"); }"#;
        assert!(run(src).is_empty());
    }
}
