//! no-unverified-hostname AST backend — disabled TLS hostname verification.
//!
//! Walks `pair` nodes whose key is `checkServerIdentity` and whose value is
//! either `null`, an arrow function, or a plain function expression. A
//! caller-supplied named callback (e.g. `checkServerIdentity: verifyHost`)
//! is allowed because the verifier may actually enforce identity checks.

use crate::diagnostic::{Diagnostic, Severity};

/// Strip surrounding quotes from a property-name node text.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c| c == '"' || c == '\'' || c == '`')
}

crate::ast_check! { on ["pair"] prefilter = ["checkServerIdentity"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return };
    let Ok(key_text) = key.utf8_text(source) else { return };
    if unquote(key_text) != "checkServerIdentity" {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    let kind = value.kind();
    let is_disabled = matches!(
        kind,
        "null" | "arrow_function" | "function_expression" | "function"
    );
    if !is_disabled {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-unverified-hostname".into(),
        message: "`checkServerIdentity` override disables TLS hostname verification.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_arrow_noop() {
        assert_eq!(run_on("const x = { checkServerIdentity: () => {} };").len(), 1);
    }

    #[test]
    fn flags_function_noop() {
        assert_eq!(
            run_on("const x = { checkServerIdentity: function() {} };").len(),
            1
        );
    }

    #[test]
    fn flags_null() {
        assert_eq!(run_on("const x = { checkServerIdentity: null };").len(), 1);
    }

    #[test]
    fn allows_proper_check() {
        assert!(run_on("const x = { checkServerIdentity: verifyHost };").is_empty());
    }

    #[test]
    fn allows_unrelated() {
        assert!(run_on("const x = tls.connect({ host: 'example.com' });").is_empty());
    }
}
