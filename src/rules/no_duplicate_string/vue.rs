//! no-duplicate-string — Vue SFC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;
use crate::rules::vue_sfc::{self, ScriptBlock};
use crate::rules::walker::collect_nodes_of_kinds;
use std::collections::HashMap;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let blocks = vue_sfc::extract_scripts(tree, ctx.source);
        if blocks.is_empty() {
            return Vec::new();
        }
        let min_length = ctx.config.threshold("no-duplicate-string", "min_length", ctx.lang);
        let min_occurrences = ctx
            .config
            .threshold("no-duplicate-string", "min_occurrences", ctx.lang);
        // Count occurrences across ALL <script> blocks of this SFC so
        // a string used in both the regular `<script>` and in
        // `<script setup>` counts as two.
        let mut occurrences: HashMap<String, Vec<(ScriptBlock<'_>, tree_sitter::Point)>> =
            HashMap::new();
        let mut inner_trees: Vec<(ScriptBlock<'_>, tree_sitter::Tree)> = Vec::new();
        for block in blocks {
            let mut parser = tree_sitter::Parser::new();
            if parser
                .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
                .is_err()
            {
                continue;
            }
            let Some(inner_tree) = parser.parse(block.text, None) else {
                continue;
            };
            inner_trees.push((block, inner_tree));
        }
        for (block, inner_tree) in &inner_trees {
            let source_bytes = block.text.as_bytes();
            for node in collect_nodes_of_kinds(inner_tree, TS_STRING_KINDS) {
                let Ok(raw) = node.utf8_text(source_bytes) else {
                    continue;
                };
                let content = super::strip_string_delimiters(raw);
                if content.chars().count() < min_length {
                    continue;
                }
                if super::should_ignore_string_node(node, source_bytes) {
                    continue;
                }
                occurrences
                    .entry(content.to_string())
                    .or_default()
                    .push((block.clone(), node.start_position()));
            }
        }
        let mut diagnostics = Vec::new();
        for (content, hits) in &occurrences {
            if hits.len() < min_occurrences {
                continue;
            }
            for (block, pos) in &hits[min_occurrences - 1..] {
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
                    rule_id: "no-duplicate-string".into(),
                    message: format!(
                        "String `\"{content}\"` appears {count} times — extract to a constant.",
                        count = hits.len()
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics.sort_by_key(|d| (d.line, d.column));
        diagnostics
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
    fn flags_duplicate_string_in_vue_script() {
        let src = "<script>\nconst a = \"hello world\";\nconst b = \"hello world\";\nconst c = \"hello world\";\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_short_string() {
        let src =
            "<script>\nconst a = \"short\";\nconst b = \"short\";\nconst c = \"short\";\n</script>";
        assert!(run(src).is_empty());
    }
}
