//! no-unverified-certificate backend for Rust.
//!
//! Walks `call_expression` nodes for:
//! - `danger_accept_invalid_certs(true)` (reqwest)
//! - `set_verify(SslVerifyMode::NONE)` / `set_verify(SSL_VERIFY_NONE)` (openssl)
//! - `dangerous().set_certificate_verifier(...)` (rustls)
//!
//! Disabling TLS certificate verification enables MITM attacks.
//!
//! The `danger_accept_invalid_certs` / `danger_accept_invalid_hostnames`
//! toggles are flagged only when they disable verification *by default*:
//! a hardcoded `true` argument on an unconditional path. Passing a
//! runtime-controlled value (a variable, field, or parameter), or gating a
//! hardcoded `true` behind an `if` that tests such a value, exposes an
//! opt-in escape hatch the caller must explicitly request — that is a
//! capability, not a vulnerable default — so it is not flagged.

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

/// True if the call's sole argument is a hardcoded `true` boolean literal.
///
/// A runtime-controlled argument (variable, field, parameter, or any other
/// expression) is *not* a hardcoded insecure default — the caller decides at
/// runtime whether to disable verification — so only the literal `true`
/// disables verification unconditionally. A `const`-bound `true` passed by
/// name reads as an identifier and is treated as runtime-controlled (not
/// flagged); covering that compile-time case would require constant resolution
/// and is out of scope.
fn arg_is_hardcoded_true(call: tree_sitter::Node, source: &[u8]) -> bool {
    let args = argument_nodes(call);
    let [arg] = args.as_slice() else {
        return false;
    };
    arg.kind() == "boolean_literal" && arg.utf8_text(source) == Ok("true")
}

/// True if `node` is nested inside the consequence of an `if` whose condition
/// references a runtime value (an identifier or a field access).
///
/// This recognises the opt-in escape-hatch pattern where a hardcoded
/// `danger_accept_invalid_certs(true)` lives inside `if config.insecure { … }`:
/// the disable is reachable only when the caller's runtime flag is set, so it
/// is a gated capability rather than a vulnerable default. An `if` whose
/// condition is itself a constant (`if true`, `if cfg!(...)`) is not a runtime
/// gate and does not exempt the call.
fn is_gated_by_runtime_condition(node: tree_sitter::Node) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "if_expression" => {
                // Only the consequence is gated; the `else` branch runs when
                // the flag is *off* and must not be exempted.
                let in_consequence = parent
                    .child_by_field_name("consequence")
                    .is_some_and(|c| c == cur);
                if in_consequence
                    && let Some(condition) = parent.child_by_field_name("condition")
                    && condition_references_runtime_value(condition)
                {
                    return true;
                }
            }
            // Stop at a function/closure boundary: a gate must enclose the
            // call within the same body.
            "function_item" | "closure_expression" => return false,
            _ => {}
        }
        cur = parent;
    }
    false
}

/// True if the `if` condition tree contains an `identifier` or `field_expression`
/// — i.e. it depends on a runtime value rather than only compile-time constants.
fn condition_references_runtime_value(condition: tree_sitter::Node) -> bool {
    let mut stack = vec![condition];
    while let Some(current) = stack.pop() {
        match current.kind() {
            "field_expression" | "identifier" => return true,
            // A macro condition like `cfg!(...)` is compile-time, not runtime.
            "macro_invocation" => continue,
            _ => {}
        }
        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(name) = method_name(node, source) else { return };

    let is_violation = match name {
        "danger_accept_invalid_certs" | "danger_accept_invalid_hostnames" => {
            // Flag only a hardcoded `true` that disables verification by
            // default. A runtime-controlled argument, or a hardcoded `true`
            // gated behind a runtime `if`, is an opt-in escape hatch.
            arg_is_hardcoded_true(node, source) && !is_gated_by_runtime_condition(node)
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

    #[test]
    fn allows_runtime_controlled_field_argument() {
        // Issue #5028: the argument is a runtime config field, so the caller
        // opts in at runtime — not a hardcoded insecure default.
        assert!(
            run_on(
                "fn f() { builder.danger_accept_invalid_certs(self.disable_verification); }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_runtime_controlled_variable_argument() {
        assert!(
            run_on("fn f(insecure: bool) { builder.danger_accept_invalid_certs(insecure); }")
                .is_empty()
        );
    }

    #[test]
    fn allows_hardcoded_true_gated_by_runtime_if() {
        // Issue #5028 (ureq native_tls.rs): hardcoded `true` reachable only
        // when the caller's runtime flag is set.
        let src = "fn f(tls_config: &TlsConfig) {
            if tls_config.disable_verification {
                builder.danger_accept_invalid_certs(true);
                builder.danger_accept_invalid_hostnames(true);
            }
        }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_hardcoded_true_in_else_of_runtime_if() {
        // The disable runs when the flag is *off* — a vulnerable default.
        let src = "fn f(tls_config: &TlsConfig) {
            if tls_config.secure {
                let _ = 1;
            } else {
                builder.danger_accept_invalid_certs(true);
            }
        }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_hardcoded_hostnames_true() {
        assert_eq!(
            run_on("fn f() { builder.danger_accept_invalid_hostnames(true); }").len(),
            1,
        );
    }
}
