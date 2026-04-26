//! sql-no-offset-pagination — Vue SFC backend.
//!
//! Same approach as the other SQL-rule Vue backends: walk the outer
//! Vue tree for `<script>` blocks, re-parse each one with the
//! TypeScript grammar, and run the same string-walk logic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_string, TS_STRING_KINDS};
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
        if !is_sql_string(text) {
            continue;
        }
        if !super::sql_uses_offset_pagination(text) {
            continue;
        }
        let pos = node.start_position();
        let file_row = pos.row + block.start_row;
        let file_col = if pos.row == 0 {
            pos.column + block.start_column
        } else {
            pos.column
        };
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: file_row + 1,
            column: file_col + 1,
            rule_id: "sql-no-offset-pagination".into(),
            message: "`OFFSET` pagination is O(N) on deep pages — use \
                      cursor-based pagination: \
                      `WHERE id > :last_id ORDER BY id LIMIT N`."
                .into(),
            severity: Severity::Warning,
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
    fn flags_pagination_in_vue_script() {
        let src = "<script>\nconst q = `SELECT * FROM users LIMIT 10 OFFSET 100`;\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_string_array_in_vue_script() {
        let src =
            "<script>\nconst bases = [\"delay\", \"offset\", \"limit\", \"rate\"];\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_template_html() {
        let src = "<template>\n  <p>limit and offset</p>\n</template>";
        assert!(run(src).is_empty());
    }
}
