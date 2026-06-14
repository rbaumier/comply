//! no-clear-text-protocol — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::RUST_STRING_KINDS;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(RUST_STRING_KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.path_segments.in_test_dir
            || crate::rules::rust_helpers::is_under_tests_dir(ctx.path)
        {
            return;
        }
        let source_bytes = ctx.source.as_bytes();
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        let Some(prefix) = super::is_clear_text_url(text) else {
            return;
        };
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-clear-text-protocol".into(),
            message: format!(
                "Clear-text protocol `{prefix}` detected — use the encrypted equivalent."
            ),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
    }

    #[test]
    fn flags_http_url_in_string_literal() {
        let src = r#"fn f() { let u = "http://api.acme.io"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_http_url_in_raw_string_literal() {
        let src = r###"fn f() { let u = r#"http://api.acme-prod.io/path"#; }"###;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_localhost() {
        let src = r#"fn f() { let u = "http://localhost:3000"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_bare_prefix_in_contains_check() {
        // The user's exact FP — port to Rust idioms.
        let src = r#"
            fn check(text: &str) -> bool {
                text.contains("http://") || text.contains("https://")
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_bare_prefix_constant() {
        let src = r#"const HTTP_PREFIX: &str = "http://";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_url_in_comment() {
        let src = "// see http://api.acme.io\nfn f() {}";
        assert!(run(src).is_empty());
    }

    // #1260 — axum's `test_helpers/test_client.rs` interpolates a loopback
    // `SocketAddr` into `http://{addr}`. TLS is not viable for in-process test
    // servers, so the whole test-helper file is exempt.
    #[test]
    fn does_not_flag_http_in_test_helpers_dir() {
        let src = r#"fn get(&self) { self.client.get(format!("http://{}{url}", self.addr)); }"#;
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            src,
            "axum/src/test_helpers/test_client.rs",
        );
        assert!(diags.is_empty());
    }

    // #1260 negative space — a concrete external host in production code still fires.
    #[test]
    fn still_flags_external_host_in_production_file() {
        let src = r#"fn f() { let u = "http://example.test"; let _ = u; }"#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/client.rs");
        assert!(diags.is_empty(), "guard sanity: example.test is exempt");
        let src = r#"fn f() { let u = "http://api.acme.io"; let _ = u; }"#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/client.rs");
        assert_eq!(diags.len(), 1);
    }
}
