//! no-clear-text-protocol — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::RUST_STRING_KINDS;
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        for node in collect_nodes_of_kinds(tree, RUST_STRING_KINDS) {
            let Ok(text) = node.utf8_text(source_bytes) else {
                continue;
            };
            let Some(prefix) = super::is_clear_text_url(text) else {
                continue;
            };
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-clear-text-protocol".into(),
                message: format!(
                    "Clear-text protocol `{prefix}` detected — use the encrypted equivalent."
                ),
                severity: Severity::Error,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(src, &Check)
    }

    #[test]
    fn flags_http_url_in_string_literal() {
        let src = r#"fn f() { let u = "http://example.com"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_http_url_in_raw_string_literal() {
        let src = r###"fn f() { let u = r#"http://api.example.com/path"#; }"###;
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
        let src = "// see http://example.com\nfn f() {}";
        assert!(run(src).is_empty());
    }
}
