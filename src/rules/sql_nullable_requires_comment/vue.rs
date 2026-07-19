//! sql-nullable-requires-comment — Vue SFC backend.
//!
//! Only string literals inside `<script>` blocks are inspected, so HTML
//! `<template>` markup (e.g. `<th class="text-left">`, `<td>`) is never
//! mistaken for a nullable SQL column definition.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{TS_STRING_KINDS, is_sql_ddl};
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
        if !is_sql_ddl(text) {
            continue;
        }
        let node_row = node.start_position().row;
        for line_offset in super::nullable_lines_without_comment(text) {
            let file_row = node_row + line_offset + block.start_row;
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: file_row + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Nullable column has no comment explaining why NULL is allowed.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
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
    fn flags_nullable_in_vue_script() {
        let src = "<script>\nconst m = `CREATE TABLE users (\n  deleted_at TIMESTAMP,\n)`;\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_not_null_in_vue_script() {
        let src =
            "<script>\nconst m = `CREATE TABLE users (\n  email TEXT NOT NULL,\n)`;\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_html_table_template_issue_4882() {
        // Regression for #4882 (vuetifyjs/vuetify Tooling.vue): an HTML
        // `<table>` with `<th class="text-left">` header cells plus an i18n
        // key `tools.create.type` defeats the flat-text DDL heuristic. The
        // Vue backend only inspects `<script>` string literals, so the HTML
        // template must not be treated as a nullable SQL column.
        let src = "\
<template>
  <v-table density=\"comfortable\" hover>
    <thead>
      <tr>
        <th class=\"text-left\">{{ t('home.tooling.headers.tool') }}</th>
        <th class=\"text-left\">{{ t('home.tooling.headers.type') }}</th>
        <th class=\"text-left d-none d-sm-table-cell\">{{ t('home.tooling.headers.description') }}</th>
      </tr>
    </thead>
  </v-table>
</template>
<script setup>
const tools = [{ type: t('home.tooling.tools.create.type') }];
</script>";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }
}
