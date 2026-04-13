//! justify-inaction Vue SFC backend.
//!
//! Extracts the `<script>` blocks and reparses each one with the
//! TypeScript grammar, then runs the TS logic on the inner tree.
//! Diagnostic coordinates are translated back to the SFC file.

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

    let mut cursor = inner_tree.root_node().walk();
    let mut stack: Vec<tree_sitter::Node> = vec![inner_tree.root_node()];
    while let Some(node) = stack.pop() {
        inspect_node(node, block, ctx, diagnostics);
        cursor.reset(node);
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn inspect_node(
    node: tree_sitter::Node,
    block: &ScriptBlock<'_>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match node.kind() {
        "catch_clause" => check_field_body(node, "body", "catch", block, ctx, diagnostics),
        "finally_clause" => {
            check_field_body(node, "body", "finally", block, ctx, diagnostics);
        }
        "if_statement" => {
            check_field_body(node, "consequence", "if", block, ctx, diagnostics);
        }
        "else_clause" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "statement_block" {
                    flag_empty(node, child, "else", block, ctx, diagnostics);
                    break;
                }
            }
        }
        "while_statement" => check_field_body(node, "body", "while", block, ctx, diagnostics),
        "do_statement" => {
            check_field_body(node, "body", "do-while", block, ctx, diagnostics);
        }
        "for_statement" => check_field_body(node, "body", "for", block, ctx, diagnostics),
        "for_in_statement" => {
            check_field_body(node, "body", "for-in", block, ctx, diagnostics);
        }
        "for_of_statement" => {
            check_field_body(node, "body", "for-of", block, ctx, diagnostics);
        }
        "switch_default" => {
            let body_empty = match node.child_by_field_name("body") {
                Some(b) if b.kind() == "statement_block" => b.named_child_count() == 0,
                _ => node.named_child_count() == 0,
            };
            if body_empty {
                push_diag(node, "default", block, ctx, diagnostics);
            }
        }
        _ => {}
    }
}

fn check_field_body(
    node: tree_sitter::Node,
    field: &str,
    what: &str,
    block: &ScriptBlock<'_>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(body) = node.child_by_field_name(field) {
        flag_empty(node, body, what, block, ctx, diagnostics);
    }
}

fn flag_empty(
    container: tree_sitter::Node,
    body: tree_sitter::Node,
    what: &str,
    block: &ScriptBlock<'_>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if body.kind() != "statement_block" || body.named_child_count() != 0 {
        return;
    }
    push_diag(container, what, block, ctx, diagnostics);
}

fn push_diag(
    node: tree_sitter::Node,
    what: &str,
    block: &ScriptBlock<'_>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let pos = node.start_position();
    let file_row = pos.row + block.start_row;
    let file_col = if pos.row == 0 {
        pos.column + block.start_column
    } else {
        pos.column
    };
    let msg = if what == "default" {
        "Empty `default` case \u{2014} add a comment inside explaining why the inaction is intentional.".to_string()
    } else {
        format!(
            "Empty `{what}` block \u{2014} add a comment inside explaining why the inaction is intentional."
        )
    };
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: file_row + 1,
        column: file_col + 1,
        rule_id: "justify-inaction".into(),
        message: msg,
        severity: Severity::Warning,
        span: None,
    });
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
    fn flags_empty_catch_in_vue_script() {
        let src = "<script>\ntry { x(); } catch (e) {}\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_commented_catch_in_vue_script() {
        let src = "<script>\ntry { x(); } catch (e) { /* swallowed */ }\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_empty_while_in_vue_script_setup() {
        let src = "<script setup lang=\"ts\">\nwhile (poll()) {}\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_empty_arrow_in_vue_script() {
        let src = "<script>\nconst noop = () => {};\n</script>";
        assert!(run(src).is_empty());
    }
}
