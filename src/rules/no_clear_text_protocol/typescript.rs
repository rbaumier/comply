//! no-clear-text-protocol — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(TS_STRING_KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
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
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_http_url() {
        assert_eq!(run(r#"const url = "http://example.com";"#).len(), 1);
    }

    #[test]
    fn flags_ftp_url() {
        assert_eq!(run(r#"const url = "ftp://files.example.com";"#).len(), 1);
    }

    #[test]
    fn flags_template_literal_with_host() {
        let src = r"const u = `http://api.example.com/${path}`;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_https() {
        assert!(run(r#"const url = "https://example.com";"#).is_empty());
    }

    #[test]
    fn does_not_flag_localhost() {
        assert!(run(r#"const url = "http://localhost:3000";"#).is_empty());
    }

    #[test]
    fn does_not_flag_loopback() {
        assert!(run(r#"const url = "http://127.0.0.1:8080";"#).is_empty());
    }

    #[test]
    fn does_not_flag_bare_prefix_in_detection_logic() {
        // The user's exact FP family — `"http://"` here is a needle
        // for substring matching, not a URL value.
        let src = r#"if (text.includes("http://") || text.includes("https://")) {}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_bare_prefix_constant() {
        let src = r#"const HTTP_PREFIX = "http://";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_url_in_comment() {
        // Comments are never visited by the AstCheck walk.
        let src = "// see http://example.com for details\nconst x = 1;";
        assert!(run(src).is_empty());
    }
}
