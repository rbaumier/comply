//! no-weak-hashing backend for Rust.
//!
//! Flags MD5/SHA1 usage via identifiers like `Md5::new()`, `Sha1::new()`,
//! or string literals containing these algorithm names in crypto contexts.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{enclosing_fn, subtree_string_literal_contains};

const WEAK_HASH_TYPES: &[&str] = &["Md5", "Sha1", "MD5", "SHA1"];

/// RFC 6455 §1.3 globally-unique identifier concatenated with the
/// `Sec-WebSocket-Key` before the SHA-1 digest of the opening handshake. The
/// constant is fixed by the protocol and present in every conformant WebSocket
/// implementation, so a SHA-1 construction in a function carrying this literal
/// is the protocol-mandated handshake digest, not a security choice.
const WEBSOCKET_HANDSHAKE_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");

    // Match `Md5::new()`, `Sha1::new()`, `md5::compute()`, `sha1::Sha1::new()`
    for &weak in WEAK_HASH_TYPES {
        let weak_lower = weak.to_ascii_lowercase();
        let callee_lower = callee_text.to_ascii_lowercase();
        if callee_lower.starts_with(&format!("{weak_lower}::"))
            || callee_lower.contains(&format!("::{weak_lower}::"))
        {
            // Exempt SHA-1 used for the RFC 6455 WebSocket opening handshake:
            // when the enclosing function carries the protocol's GUID literal,
            // SHA-1 is the wire-format digest the RFC mandates, not a chosen
            // crypto primitive. MD5 has no analogous protocol use — narrow the
            // exemption to SHA-1.
            if weak_lower == "sha1"
                && enclosing_fn(node).is_some_and(|f| {
                    subtree_string_literal_contains(f, source, WEBSOCKET_HANDSHAKE_GUID)
                })
            {
                return;
            }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-weak-hashing".into(),
                message: format!(
                    "Weak hashing algorithm `{callee_text}` — use SHA-256 or stronger.",
                ),
                severity: Severity::Error,
                span: None,
            });
            return;
        }
    }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_md5_new() {
        assert_eq!(run_on("fn f() { let h = Md5::new(); }").len(), 1);
    }

    #[test]
    fn flags_sha1_new() {
        assert_eq!(run_on("fn f() { let h = Sha1::new(); }").len(), 1);
    }

    #[test]
    fn flags_md5_compute() {
        assert_eq!(run_on("fn f() { let h = md5::compute(data); }").len(), 1);
    }

    #[test]
    fn allows_sha256() {
        assert!(run_on("fn f() { let h = Sha256::new(); }").is_empty());
    }

    // Regression for #3241: SHA-1 in the RFC 6455 WebSocket opening handshake,
    // identified by the protocol GUID literal in the same function, is the
    // mandated wire-format digest, not a crypto choice.
    #[test]
    fn allows_sha1_in_rfc6455_websocket_handshake() {
        let src = r#"fn sign(key: &[u8]) -> HeaderValue {
    use base64::engine::Engine as _;

    let mut sha1 = Sha1::default();
    sha1.update(key);
    sha1.update(&b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11"[..]);
    let b64 = Bytes::from(base64::engine::general_purpose::STANDARD.encode(sha1.finalize()));
    HeaderValue::from_maybe_shared(b64).expect("base64 is a valid value")
}"#;
        assert!(run_on(src).is_empty());
    }

    // The GUID is recognized case-insensitively (it is uppercase hex in the
    // RFC, but a lowercase spelling is the same protocol constant).
    #[test]
    fn allows_sha1_in_handshake_lowercase_guid() {
        let src = r#"fn sign(key: &[u8]) {
    let mut sha1 = Sha1::default();
    sha1.update(&b"258eafa5-e914-47da-95ca-c5ab0dc85b11"[..]);
}"#;
        assert!(run_on(src).is_empty());
    }

    // True positive preserved: SHA-1 with no handshake GUID in scope still fires.
    #[test]
    fn flags_sha1_default_without_guid() {
        assert_eq!(run_on("fn f() { let h = Sha1::default(); }").len(), 1);
    }

    // The GUID only exempts SHA-1; MD5 has no analogous protocol use and stays
    // flagged even alongside the literal.
    #[test]
    fn flags_md5_even_with_handshake_guid() {
        let src = r#"fn f() {
    let _g = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    let h = Md5::new();
}"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
