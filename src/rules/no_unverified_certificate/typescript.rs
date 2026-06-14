//! no-unverified-certificate AST backend — disabled SSL cert verification.
//!
//! Walks `pair` nodes (object property assignments) for keys
//! `rejectUnauthorized` / `verify` whose value is the literal `false`, and
//! `assignment_expression` nodes whose left-hand side ends in
//! `NODE_TLS_REJECT_UNAUTHORIZED`.

use crate::diagnostic::{Diagnostic, Severity};

const FALSY_REJECT_KEYS: &[&str] = &["rejectUnauthorized", "verify"];

/// Strip surrounding quotes from a string-literal node text.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c| c == '"' || c == '\'' || c == '`')
}

/// True when an AST node represents the boolean literal `false`.
fn is_false_literal(value: tree_sitter::Node, source: &[u8]) -> bool {
    if value.kind() == "false" {
        return true;
    }
    if value.kind() == "string" {
        let Ok(text) = value.utf8_text(source) else {
            return false;
        };
        let inner = unquote(text);
        return inner == "0" || inner.eq_ignore_ascii_case("false");
    }
    false
}

/// True when `node` reads the `NODE_TLS_REJECT_UNAUTHORIZED` env var name
/// (used as the LHS of an assignment that disables verification).
fn is_node_tls_lhs(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else {
        return false;
    };
    text.contains("NODE_TLS_REJECT_UNAUTHORIZED")
}

crate::ast_check! { on ["pair", "assignment_expression"] prefilter = ["rejectUnauthorized", "NODE_TLS_REJECT_UNAUTHORIZED", "verify"] => |node, source, ctx, diagnostics|
    let kind = node.kind();

    if kind == "pair" {
        let Some(key) = node.child_by_field_name("key") else { return };
        let Ok(key_text) = key.utf8_text(source) else { return };
        let key_name = unquote(key_text);
        if !FALSY_REJECT_KEYS.contains(&key_name) {
            return;
        }
        let Some(value) = node.child_by_field_name("value") else { return };
        if !is_false_literal(value, source) {
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
        return;
    }

    if kind == "assignment_expression" {
        let Some(lhs) = node.child_by_field_name("left") else { return };
        if !is_node_tls_lhs(lhs, source) {
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_reject_unauthorized_false() {
        assert_eq!(run_on("const x = { rejectUnauthorized: false };").len(), 1);
    }

    #[test]
    fn flags_node_tls_env() {
        assert_eq!(
            run_on("process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0'").len(),
            1
        );
    }

    #[test]
    fn flags_verify_false() {
        assert_eq!(run_on("const x = { verify: false };").len(), 1);
    }

    #[test]
    fn allows_reject_unauthorized_true() {
        assert!(run_on("const x = { rejectUnauthorized: true };").is_empty());
    }

    #[test]
    fn allows_verify_true() {
        assert!(run_on("const x = { verify: true };").is_empty());
    }

    // Regression for #1511: self-signed certs in a local HTTPS test server are
    // intentional, so `skip_in_test_dir` must suppress the rule in test files.
    const ISSUE_1511_SOURCE: &str = r#"undici.setGlobalDispatcher(
  new undici.Agent({
    allowH2: true,
    connect: {
      rejectUnauthorized: false,
    },
  }),
);"#;

    #[test]
    fn skips_self_signed_cert_in_test_dir() {
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            ISSUE_1511_SOURCE,
            "packages/tests/server/adapters/standalone.http2.test.ts",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn still_flags_self_signed_cert_in_source_dir() {
        let diags = crate::rules::test_helpers::run_rule_gated(&Check, ISSUE_1511_SOURCE, "src/client.ts");
        assert_eq!(diags.len(), 1);
    }
}
