//! no-commented-out-code — TS / JS / TSX backend.
//!
//! Walks every `comment` node in the parsed source, groups adjacent
//! ones, strips the `//` / `/* */` delimiters, and re-parses the body
//! as TypeScript. A clean parse (no errors) that contains at least one
//! rich construct (declaration, call, assignment, control flow) means
//! the comment is very likely commented-out code.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut comments = crate::rules::walker::collect_nodes_of_kinds(tree, &["comment"]);
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
            if !parses_as_typescript_code(&body) {
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
                span: None,
            });
        }
        diagnostics
    }
}

/// Build a single text block from a group of adjacent comments.
/// Returns None if the group only contained doc comments.
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

/// Re-parse `body` with the TypeScript grammar. Returns true if the
/// inner tree has no error nodes AND contains at least one rich
/// code construct.
pub(super) fn parses_as_typescript_code(body: &str) -> bool {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .is_err()
    {
        return false;
    }
    let Some(tree) = parser.parse(body, None) else {
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
        if is_rich_ts_kind(node.kind()) {
            found = true;
        }
    });
    found
}

fn is_rich_ts_kind(kind: &str) -> bool {
    matches!(
        kind,
        "call_expression"
            | "assignment_expression"
            | "augmented_assignment_expression"
            | "lexical_declaration"
            | "variable_declaration"
            | "function_declaration"
            | "function_expression"
            | "generator_function_declaration"
            | "arrow_function"
            | "if_statement"
            | "for_statement"
            | "for_in_statement"
            | "while_statement"
            | "do_statement"
            | "return_statement"
            | "throw_statement"
            | "try_statement"
            | "switch_statement"
            | "class_declaration"
            | "interface_declaration"
            | "type_alias_declaration"
            | "enum_declaration"
            | "import_statement"
            | "export_statement"
            | "new_expression"
            | "update_expression"
            | "await_expression"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_commented_const() {
        assert_eq!(run("// const x = 5;").len(), 1);
    }

    #[test]
    fn flags_commented_function_call() {
        assert_eq!(run("// foo(bar);").len(), 1);
    }

    #[test]
    fn flags_adjacent_commented_lines() {
        let src = "// const x = 5;\n// const y = 10;";
        assert_eq!(run(src).len(), 1, "adjacent comments should produce ONE diagnostic");
    }

    #[test]
    fn allows_prose_comment() {
        assert!(run("// This function computes the total cost.").is_empty());
    }

    #[test]
    fn allows_triple_slash_doc_comment() {
        assert!(run("/// Returns the parsed result.").is_empty());
    }

    #[test]
    fn allows_short_label_comment() {
        assert!(run("// setup").is_empty());
    }

    #[test]
    fn allows_pattern_list_prose() {
        // The user's reported false positive: a comment describing syntax
        // patterns like `const NAME =, let NAME =, var NAME =` — these end
        // with `=` without a RHS, so the TS parser returns errors and the
        // group is NOT flagged.
        assert!(run("// const foo =, let foo =, var foo =").is_empty());
    }

    #[test]
    fn allows_inline_syntax_description() {
        assert!(run("// const foo =").is_empty());
    }

    #[test]
    fn flags_commented_block_comment() {
        assert_eq!(run("/* const x = 5; foo(x); */").len(), 1);
    }

    #[test]
    fn allows_block_comment_prose() {
        assert!(run("/* this explains what follows */").is_empty());
    }

    #[test]
    fn allows_jsdoc_block_comment() {
        assert!(run("/** @returns the cost */").is_empty());
    }

    #[test]
    fn non_adjacent_comments_produce_separate_diagnostics() {
        // Two comments with real code between them — each is its own group.
        let src = "// const x = 5;\nconst y = 10;\n// foo(y);";
        assert_eq!(run(src).len(), 2);
    }
}
