//! max-function-lines backend for TypeScript / JavaScript / TSX.
//!
//! Walks every function-like node and flags those spanning more than 30 lines.
//! `saturating_sub` guards against malformed nodes where `end.row < start.row`
//! (rare, can happen inside parse-error recovery).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

/// Default threshold: 30 lines ≈ one code-review screen without
/// scrolling. The user can override this in `comply.toml` via
/// `[rules.max-function-lines] max = N`.
pub const DEFAULT_MAX_LINES: usize = 30;

/// Tree-sitter node kinds representing a function body in the TS grammar.
const TS_FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "method_definition",
    "arrow_function",
    "generator_function_declaration",
];

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let max_lines = ctx
            .config
            .threshold("max-function-lines", "max", DEFAULT_MAX_LINES);
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if let Some(d) = check_function_node(node, source_bytes, ctx.path, max_lines) {
                diagnostics.push(d);
            }
        });
        diagnostics
    }
}

/// Build a diagnostic for one AST node if it's a function over `max_lines`.
fn check_function_node(
    node: tree_sitter::Node,
    source: &[u8],
    path: &std::path::Path,
    max_lines: usize,
) -> Option<Diagnostic> {
    if !TS_FUNCTION_KINDS.contains(&node.kind()) {
        return None;
    }
    let start = node.start_position();
    let end = node.end_position();
    let line_count = end.row.saturating_sub(start.row) + 1;
    if line_count <= max_lines {
        return None;
    }
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>");
    Some(Diagnostic {
        path: path.to_path_buf(),
        line: start.row + 1,
        column: start.column + 1,
        rule_id: "max-function-lines".into(),
        message: format!(
            "Function '{name}' is {line_count} lines (max {max_lines}). \
             Extract a named helper for the logic below line {}.",
            start.row + 1 + max_lines
        ),
        severity: Severity::Error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.ts"), source),
            &tree,
        )
    }

    #[test]
    fn flags_long_function() {
        let body = "let x = 0;\n".repeat(DEFAULT_MAX_LINES + 5);
        let diags = run_on(&format!("function long() {{\n{body}}}"));
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_short_function() {
        assert!(run_on("function short() { return 42; }").is_empty());
    }

    #[test]
    fn extracts_function_name_in_message() {
        let body = "let x = 0;\n".repeat(DEFAULT_MAX_LINES + 1);
        let diags = run_on(&format!("function myLongFunc() {{\n{body}}}"));
        assert!(diags[0].message.contains("myLongFunc"));
    }
}
