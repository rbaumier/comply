//! detect-option-rejectunauthorized backend — flag
//! `{ rejectUnauthorized: false }` object properties.

use crate::diagnostic::{Diagnostic, Severity};

fn key_text<'a>(key: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    // Key can be a plain property_identifier or a string literal.
    let text = key.utf8_text(source).unwrap_or("");
    text.trim_matches(|c| c == '"' || c == '\'')
}

crate::ast_check! { on ["pair"] prefilter = ["rejectUnauthorized"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return };
    let Some(value) = node.child_by_field_name("value") else { return };
    if key_text(key, source) != "rejectUnauthorized" {
        return;
    }
    if value.kind() != "false" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "detect-option-rejectunauthorized".into(),
        message: "`rejectUnauthorized: false` disables TLS certificate validation — remove it.".into(),
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_reject_unauthorized_false() {
        let source = "const opts = { rejectUnauthorized: false };";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_string_key() {
        let source = r#"const opts = { "rejectUnauthorized": false };"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_reject_unauthorized_true() {
        let source = "const opts = { rejectUnauthorized: true };";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_other_option_false() {
        let source = "const opts = { somethingElse: false };";
        assert!(run_on(source).is_empty());
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
