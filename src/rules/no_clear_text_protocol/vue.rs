//! no-clear-text-protocol — Vue SFC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;
use crate::rules::vue_sfc::{self, ScriptBlock};
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let blocks = vue_sfc::extract_scripts(tree, ctx.source);
        let mut diagnostics = Vec::new();
        for block in blocks {
            lint_block(&block, ctx, &mut diagnostics);
        }
        diagnostics
    }
}

fn lint_block(block: &ScriptBlock<'_>, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .is_err()
    {
        return;
    }
    let Some(inner_tree) = parser.parse(block.text, None) else {
        return;
    };
    let source_bytes = block.text.as_bytes();
    for node in collect_nodes_of_kinds(&inner_tree, TS_STRING_KINDS) {
        let Ok(text) = node.utf8_text(source_bytes) else {
            continue;
        };
        let Some(prefix) = super::is_clear_text_url(text) else {
            continue;
        };
        let pos = node.start_position();
        let file_row = pos.row + block.start_row;
        let file_col = if pos.row == 0 {
            pos.column + block.start_column
        } else {
            pos.column
        };
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: file_row + 1,
            column: file_col + 1,
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
    use crate::files::{Language, SourceFile};
    use std::path::PathBuf;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parser");
        let file = SourceFile {
            path: PathBuf::from("t.vue"),
            language: Language::Vue,
        };
        Check.check(
            &crate::rules::backend::CheckCtx::for_test(&file.path, source),
            &tree,
        )
    }

    #[test]
    fn flags_http_url_in_vue_script() {
        let src = "<script>\nconst u = \"http://api.example.com\";\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_bare_prefix_in_vue_script() {
        let src = "<script>\nif (text.includes(\"http://\")) {}\n</script>";
        assert!(run(src).is_empty());
    }
}
