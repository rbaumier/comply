//! no-unverified-certificate backend for Rust.
//!
//! Walks `call_expression` nodes for:
//! - `danger_accept_invalid_certs(true)` (reqwest)
//! - `set_verify(SslVerifyMode::NONE)` / `set_verify(SSL_VERIFY_NONE)` (openssl)
//! - `dangerous().set_certificate_verifier(...)` (rustls)
//!
//! Disabling TLS certificate verification enables MITM attacks.

use crate::diagnostic::{Diagnostic, Severity};

/// Return the method name of a call expression whose function is a
/// `field_expression` (Rust's equivalent of `.method`). For non-method
/// calls or generic call shapes returns `None`.
fn method_name<'a>(call: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "field_expression" {
        return None;
    }
    let field = func.child_by_field_name("field")?;
    field.utf8_text(source).ok()
}

/// Iterate over the named argument nodes of a `call_expression`.
fn argument_nodes<'t>(call: tree_sitter::Node<'t>) -> Vec<tree_sitter::Node<'t>> {
    let Some(args) = call.child_by_field_name("arguments") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        out.push(child);
    }
    out
}

/// True if any argument's text matches one of the unsafe markers.
fn arg_text_matches(call: tree_sitter::Node, source: &[u8], markers: &[&str]) -> bool {
    for arg in argument_nodes(call) {
        let Ok(text) = arg.utf8_text(source) else {
            continue;
        };
        let trimmed = text.trim();
        if markers.iter().any(|m| trimmed == *m || trimmed.contains(m)) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(name) = method_name(node, source) else { return };

    let is_violation = match name {
        "danger_accept_invalid_certs" | "danger_accept_invalid_hostnames" => {
            arg_text_matches(node, source, &["true"])
        }
        "set_verify" => arg_text_matches(
            node,
            source,
            &["SslVerifyMode::NONE", "SSL_VERIFY_NONE"],
        ),
        "set_certificate_verifier" => {
            // rustls: only flag when chained off `.dangerous()`.
            let func = node.child_by_field_name("function");
            let Some(field) = func else { return };
            let Some(receiver) = field.child_by_field_name("value") else { return };
            let Ok(text) = receiver.utf8_text(source) else { return };
            text.contains("dangerous()")
        }
        _ => false,
    };

    if !is_violation {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-unverified-certificate".into(),
        message: "Disabled SSL certificate verification — enables MITM attacks.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_danger_accept_invalid_certs() {
        assert_eq!(
            run_on("fn f() { client.danger_accept_invalid_certs(true); }").len(),
            1,
        );
    }

    #[test]
    fn flags_ssl_verify_mode_none() {
        assert_eq!(
            run_on("fn f() { ctx.set_verify(SslVerifyMode::NONE); }").len(),
            1,
        );
    }

    #[test]
    fn allows_normal_client() {
        assert!(run_on("fn f() { let client = Client::new(); }").is_empty());
    }
}
