//! no-commented-out-code — Rust backend.
//!
//! Same strategy as the TS backend: walk `line_comment` and
//! `block_comment` nodes, group adjacent ones, strip delimiters,
//! re-parse as Rust. A clean inner parse with at least one rich
//! construct (let declaration, call, macro, control flow) means the
//! comment is very likely commented-out code.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut comments =
            super::collect_nodes_of_kinds(tree, &["line_comment", "block_comment"]);
        comments.sort_by_key(|n| (n.start_position().row, n.start_position().column));

        let groups = super::group_adjacent(&comments);
        let mut diagnostics = Vec::new();
        for group in groups {
            let Some(body) = build_group_body(&group, source_bytes) else {
                continue;
            };
            if !super::has_code_shape(&body) {
                continue;
            }
            if !parses_as_rust_code(&body) {
                continue;
            }
            let first = group.first().copied().expect("group is non-empty");
            let pos = first.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-commented-out-code".into(),
                message: "This comment looks like commented-out code — \
                          delete it. Git history preserves the original."
                    .into(),
                severity: Severity::Warning,
            });
        }
        diagnostics
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

/// Re-parse `body` with the Rust grammar. Wraps the body in
/// `fn __probe__() { ... }` because most commented-out Rust fragments
/// are statements — they only make sense inside a function body, not
/// at module scope. A top-level `let` is a hard parse error; wrapped,
/// it's a legal `let_declaration`.
pub(super) fn parses_as_rust_code(body: &str) -> bool {
    let wrapped = format!("fn __probe__() {{\n{body}\n}}");
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .is_err()
    {
        return false;
    }
    let Some(tree) = parser.parse(&wrapped, None) else {
        return false;
    };
    let root = tree.root_node();
    if root.has_error() {
        return false;
    }
    contains_rich_code(&tree)
}

fn contains_rich_code(tree: &tree_sitter::Tree) -> bool {
    let mut found = false;
    walk_tree(tree, |node| {
        if found {
            return;
        }
        if is_rich_rust_kind(node.kind()) {
            found = true;
        }
    });
    found
}

fn is_rich_rust_kind(kind: &str) -> bool {
    matches!(
        kind,
        "let_declaration"
            | "call_expression"
            | "macro_invocation"
            | "assignment_expression"
            | "compound_assignment_expr"
            | "function_item"
            | "struct_item"
            | "enum_item"
            | "impl_item"
            | "trait_item"
            | "if_expression"
            | "match_expression"
            | "for_expression"
            | "while_expression"
            | "loop_expression"
            | "return_expression"
            | "use_declaration"
            | "break_expression"
            | "continue_expression"
            | "try_expression"
            | "await_expression"
            | "unary_expression"
            | "binary_expression"
            | "field_expression"
            | "index_expression"
            | "closure_expression"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_commented_let() {
        assert_eq!(run("// let x = 5;").len(), 1);
    }

    #[test]
    fn flags_commented_macro_call() {
        assert_eq!(run(r#"// println!("hello {}", x);"#).len(), 1);
    }

    #[test]
    fn flags_adjacent_commented_lines() {
        let src = "// let x = 5;\n// let y = 10;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_prose_comment() {
        assert!(run("// This function computes the total cost.").is_empty());
    }

    #[test]
    fn allows_triple_slash_doc() {
        assert!(run("/// Returns the parsed result.").is_empty());
    }

    #[test]
    fn allows_inner_module_doc() {
        assert!(run("//! Module-level documentation.").is_empty());
    }

    #[test]
    fn allows_pattern_list_prose() {
        // User's reported false positive, ported to Rust idioms.
        assert!(run("// let foo =, const foo =, static foo =").is_empty());
    }

    #[test]
    fn allows_short_label() {
        assert!(run("// setup").is_empty());
    }

    #[test]
    fn flags_commented_block_comment() {
        assert_eq!(run("/* let x = 5; foo(x); */").len(), 1);
    }

    #[test]
    fn allows_block_doc_comment() {
        assert!(run("/** doc */").is_empty());
    }
}
