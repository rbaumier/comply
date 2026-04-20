//! max-function-lines — TS / JS / TSX backend.
//!
//! Walks every function-like node and flags those whose body exceeds
//! 30 NCLOC (see `super::count_ncloc` for the metric). The body range
//! is the tree-sitter node's `start_position` and `end_position`, so
//! an arrow function assigned to a const is flagged exactly where the
//! `=>` lives.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

/// Tree-sitter node kinds representing a function body in the TS grammar.
const TS_FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "method_definition",
    "arrow_function",
    "generator_function_declaration",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let max_lines = ctx.config.threshold("max-function-lines", "max");
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if let Some(d) = check_function_node(ctx.source, source_bytes, node, ctx.path, max_lines)
            {
                diagnostics.push(d);
            }
        });
        diagnostics
    }
}

fn check_function_node(
    source: &str,
    source_bytes: &[u8],
    node: tree_sitter::Node,
    path: &std::path::Path,
    max_lines: usize,
) -> Option<Diagnostic> {
    if !TS_FUNCTION_KINDS.contains(&node.kind()) {
        return None;
    }
    let start = node.start_position();
    let end = node.end_position();
    let ncloc = super::count_ncloc(source, start.row, end.row);
    if ncloc <= max_lines {
        return None;
    }
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source_bytes).ok())
        .unwrap_or("<anonymous>");
    Some(Diagnostic {
        path: path.to_path_buf(),
        line: start.row + 1,
        column: start.column + 1,
        rule_id: "max-function-lines".into(),
        message: format!(
            "Function '{name}' is {ncloc} NCLOC (max {max_lines}). \
             Extract a named helper — one level of abstraction per function."
        ),
        severity: Severity::Error,
        span: None,
    })
}

/// Compute `(name, ncloc)` for every function-like node in `source`.
/// Used by `shared_tests.rs` to cross-check the TS and Rust backends
/// agree on NCLOC for equivalent fixtures.
#[cfg(test)]
pub(super) fn compute_source(source: &str) -> Vec<(String, usize)> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("grammar should load");
    let tree = parser.parse(source, None).expect("parser should produce a tree");
    let source_bytes = source.as_bytes();
    let mut out = Vec::new();
    walk_tree(&tree, |node| {
        if !TS_FUNCTION_KINDS.contains(&node.kind()) {
            return;
        }
        let start = node.start_position();
        let end = node.end_position();
        let ncloc = super::count_ncloc(source, start.row, end.row);
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source_bytes).ok())
            .unwrap_or("<anonymous>")
            .to_string();
        out.push((name, ncloc));
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_long_function() {
        let body = "let x = 0;\n".repeat(30 + 5);
        let diags = run_on(&format!("function long() {{\n{body}}}"));
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_short_function() {
        assert!(run_on("function short() { return 42; }").is_empty());
    }

    #[test]
    fn blank_lines_do_not_count() {
        // 40 blank lines + 5 code lines = 5 NCLOC, well under the threshold.
        let body = "\n".repeat(40) + &"let x = 0;\n".repeat(5);
        let diags = run_on(&format!("function stretched() {{\n{body}}}"));
        assert!(diags.is_empty());
    }

    #[test]
    fn line_comments_do_not_count() {
        // 40 lines of `// noise` + 5 real = 5 NCLOC (plus the signature line).
        let comments = "// noise\n".repeat(40);
        let real = "let x = 0;\n".repeat(5);
        let diags = run_on(&format!("function commented() {{\n{comments}{real}}}"));
        assert!(diags.is_empty());
    }

    #[test]
    fn extracts_function_name_in_message() {
        let body = "let x = 0;\n".repeat(30 + 1);
        let diags = run_on(&format!("function myLongFunc() {{\n{body}}}"));
        assert!(diags[0].message.contains("myLongFunc"));
    }
}
