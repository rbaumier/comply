//! no-commented-out-code — Vue SFC backend.
//!
//! Vue SFCs are parsed with the Vue grammar, which exposes `<script>`
//! blocks as a `script_element` containing a single `raw_text` child.
//! The grammar does NOT parse script contents as TypeScript, so this
//! backend:
//!
//! 1. Walks the Vue tree for `<script>` blocks via `vue_sfc::extract_scripts`.
//! 2. For each block, re-parses the raw text with the TypeScript
//!    grammar — same grammar the TS backend uses.
//! 3. Collects comment nodes from the inner TS tree, groups adjacent
//!    ones, runs the same mini-parse-as-code check as `typescript.rs`.
//! 4. Translates the diagnostic's (row, column) back to Vue file
//!    coordinates by adding the `<script>` block's start offset.
//!
//! HTML comments (`<!-- ... -->`) in the `<template>` section are NOT
//! examined. They rarely carry JavaScript/TypeScript, and flagging
//! them would require a different set of heuristics.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::vue_sfc::{self, ScriptBlock};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let blocks = vue_sfc::extract_scripts(tree, ctx.source);
        let mut diagnostics = Vec::new();
        for block in blocks {
            lint_script_block(&block, ctx, &mut diagnostics);
        }
        diagnostics
    }
}

fn lint_script_block(block: &ScriptBlock<'_>, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    // Re-parse the script's raw text as TypeScript. The TS grammar
    // handles plain JS too, so this covers `<script>` and
    // `<script lang="ts">` with a single parser.
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

    let mut comments = crate::rules::walker::collect_nodes_of_kinds(&inner_tree, &["comment"]);
    comments.sort_by_key(|n| (n.start_position().row, n.start_position().column));
    let groups = super::group_adjacent(&comments);

    for group in groups {
        let Some(body) = build_group_body(&group, source_bytes) else {
            continue;
        };
        if !super::has_code_shape(&body) {
            continue;
        }
        if !super::typescript::parses_as_typescript_code(&body) {
            continue;
        }
        let first = group.first().copied().expect("group is non-empty");
        let inner_pos = first.start_position();
        // Translate inner (row, column) back to Vue file coordinates:
        // - the inner tree's row 0 corresponds to the Vue file's
        //   `block.start_row`, so add it.
        // - the column on row 0 is offset by `block.start_column`; on
        //   any subsequent row, the column is already absolute in the
        //   file because lines align with the outer file exactly.
        let file_row = inner_pos.row + block.start_row;
        let file_col = if inner_pos.row == 0 {
            inner_pos.column + block.start_column
        } else {
            inner_pos.column
        };
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: file_row + 1,
            column: file_col + 1,
            rule_id: "no-commented-out-code".into(),
            message: "This comment looks like commented-out code — \
                      delete it. Git history preserves the original."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn build_group_body(group: &[tree_sitter::Node], source: &[u8]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    for node in group {
        let raw = node.utf8_text(source).ok()?;
        let Some(stripped) = super::strip_comment_syntax(raw) else {
            continue;
        };
        if !stripped.trim().is_empty() {
            lines.push(stripped);
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::{Language, SourceFile};
    use std::path::PathBuf;

    fn run(source: &str) -> Vec<Diagnostic> {
        // Vue isn't in `test_helpers`; parse inline with the Vue grammar
        // and dispatch Check manually.
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
    fn flags_commented_const_in_script_block() {
        let src = "<script>\n// const x = 5;\nconst visible = true;\n</script>";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_prose_comment_in_script_block() {
        let src =
            "<script>\n// This component mounts the widget.\nconst visible = true;\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_user_false_positive_pattern_list() {
        let src =
            "<script>\n// const foo =, let foo =, var foo =\nconst visible = true;\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn handles_script_setup_block() {
        let src = "<script setup>\n// let counter = 0;\nconst n = 1;\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn diagnostic_line_reflects_vue_file_coordinates() {
        // The `<script>` tag is on line 1 (row 0). The raw_text starts
        // on row 1 (0-indexed). `// const x = 5;` is the first line of
        // raw_text, so row 1 in Vue coordinates → `line = 2`.
        let src = "<script>\n// const x = 5;\nconst y = 1;\n</script>";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn no_diagnostics_on_empty_script() {
        assert!(run("<template><div /></template>").is_empty());
    }
}
