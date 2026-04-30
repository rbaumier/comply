//! db-no-string-concat-sql — Vue SFC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::is_sql_string;
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
    for node in collect_nodes_of_kinds(&inner_tree, &["binary_expression"]) {
        let Some(op) = node.child_by_field_name("operator") else {
            continue;
        };
        if op.utf8_text(source_bytes).unwrap_or("") != "+" {
            continue;
        }
        let Some(left) = node.child_by_field_name("left") else {
            continue;
        };
        let Some(right) = node.child_by_field_name("right") else {
            continue;
        };

        let left_sql = string_node_is_sql(left, source_bytes);
        let right_sql = string_node_is_sql(right, source_bytes);
        if !(left_sql || right_sql) {
            continue;
        }
        let other_side_dynamic = if left_sql {
            !is_string_node(right)
        } else {
            !is_string_node(left)
        };
        if !other_side_dynamic {
            continue;
        }
        let combined = node.utf8_text(source_bytes).unwrap_or("");
        if combined.contains("$1") || combined.contains("$2") {
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
            rule_id: "db-no-string-concat-sql".into(),
            message: "String concatenation with SQL keywords \
                      — SQL injection risk. Use parameterized queries \
                      (`$1`, `?`) instead."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn is_string_node(node: tree_sitter::Node) -> bool {
    matches!(node.kind(), "string" | "template_string")
}

fn string_node_is_sql(node: tree_sitter::Node, source: &[u8]) -> bool {
    if !is_string_node(node) {
        return false;
    }
    let Ok(text) = node.utf8_text(source) else {
        return false;
    };
    is_sql_string(text)
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
    fn flags_sql_concat_in_vue_script() {
        let src = "<script>\nconst q = \"SELECT * FROM users WHERE id = \" + userId;\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_non_sql_concat() {
        let src = "<script>\nconst msg = \"hello \" + name;\n</script>";
        assert!(run(src).is_empty());
    }
}
