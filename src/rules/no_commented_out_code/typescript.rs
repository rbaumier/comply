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

/// State accumulated across visits: positions (row, col, byte_start, byte_end)
/// of every `comment` node in document order. We store byte ranges (not Node)
/// because state must be `'static`-friendly owned data.
#[derive(Default)]
struct State {
    comments: Vec<CommentSpan>,
}

struct CommentSpan {
    row: usize,
    col: usize,
    end_row: usize,
    text: String,
}

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["comment"])
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(State::default()))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(state) = state.and_then(|s| s.downcast_mut::<State>()) else {
            return;
        };
        let source_bytes = ctx.source.as_bytes();
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        let pos = node.start_position();
        let end = node.end_position();
        state.comments.push(CommentSpan {
            row: pos.row,
            col: pos.column,
            end_row: end.row,
            text: text.to_string(),
        });
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(state) = state.and_then(|s| s.downcast::<State>().ok()) else {
            return;
        };
        let mut comments = state.comments;
        comments.sort_by_key(|c| (c.row, c.col));

        let groups = group_adjacent_spans(&comments);
        for group in groups {
            let Some(body) = build_group_body(&group) else {
                continue;
            };
            if !super::has_code_shape(&body) {
                continue;
            }
            if !parses_as_typescript_code(&body) {
                continue;
            }
            let first = group.first().expect("group is non-empty");
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: first.row + 1,
                column: first.col + 1,
                rule_id: "no-commented-out-code".into(),
                message: "This comment looks like commented-out code — \
                          delete it. Git history preserves the original."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Group adjacent comment spans. Mirrors `super::group_adjacent` but
/// for `CommentSpan` rather than `tree_sitter::Node`. Two comments are
/// considered adjacent if the second one starts on the same line as,
/// or the line immediately after, the first one ends.
fn group_adjacent_spans(comments: &[CommentSpan]) -> Vec<Vec<&CommentSpan>> {
    let mut groups: Vec<Vec<&CommentSpan>> = Vec::new();
    for c in comments {
        let extend = groups
            .last()
            .and_then(|g| g.last())
            .is_some_and(|last: &&CommentSpan| c.row <= last.end_row + 1);
        if extend {
            groups.last_mut().expect("last group exists").push(c);
        } else {
            groups.push(vec![c]);
        }
    }
    groups
}

/// Build a single text block from a group of adjacent comments.
/// Returns None if the group only contained doc comments.
fn build_group_body(group: &[&CommentSpan]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    for c in group {
        let Some(stripped) = super::strip_comment_syntax(&c.text) else {
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
