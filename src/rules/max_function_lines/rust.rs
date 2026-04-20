//! max-function-lines — Rust backend.
//!
//! Same semantics as the TS backend: flag every function-like node
//! whose body exceeds 30 NCLOC. Covers `function_item` (top-level,
//! impl methods, trait defaults, async) and `closure_expression`
//! (a long closure is the same smell as a long arrow function).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const RUST_FUNCTION_KINDS: &[&str] = &["function_item", "closure_expression"];

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
    if !RUST_FUNCTION_KINDS.contains(&node.kind()) {
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
        .unwrap_or("<closure>");
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
/// Used by `shared_tests.rs` to cross-check backends agree.
#[cfg(test)]
pub(super) fn compute_source(source: &str) -> Vec<(String, usize)> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("grammar should load");
    let tree = parser.parse(source, None).expect("parser should produce a tree");
    let source_bytes = source.as_bytes();
    let mut out = Vec::new();
    walk_tree(&tree, |node| {
        if !RUST_FUNCTION_KINDS.contains(&node.kind()) {
            return;
        }
        let start = node.start_position();
        let end = node.end_position();
        let ncloc = super::count_ncloc(source, start.row, end.row);
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source_bytes).ok())
            .unwrap_or("<closure>")
            .to_string();
        out.push((name, ncloc));
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_long_function_item() {
        let body = "let _ = 0;\n".repeat(30 + 5);
        let diags = run_on(&format!("fn long() {{\n{body}}}"));
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_short_function_item() {
        assert!(run_on("fn short() -> i32 { 42 }").is_empty());
    }

    #[test]
    fn blank_lines_do_not_count() {
        let body = "\n".repeat(40) + &"let _ = 0;\n".repeat(5);
        let diags = run_on(&format!("fn stretched() {{\n{body}}}"));
        assert!(diags.is_empty());
    }

    #[test]
    fn line_and_doc_comments_do_not_count() {
        let comments = "/// doc\n// note\n".repeat(20);
        let real = "let _ = 0;\n".repeat(5);
        let diags = run_on(&format!("fn commented() {{\n{comments}{real}}}"));
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_long_closure() {
        let body = "let _ = 0;\n".repeat(30 + 5);
        let src = format!("fn outer() {{ let c = || {{\n{body}}}; }}");
        let diags = run_on(&src);
        // outer fn + closure both long — expect both flagged.
        assert!(diags.iter().any(|d| d.message.contains("<closure>")));
    }

    #[test]
    fn extracts_function_name_in_message() {
        let body = "let _ = 0;\n".repeat(30 + 1);
        let diags = run_on(&format!("fn my_long_func() {{\n{body}}}"));
        assert!(diags[0].message.contains("my_long_func"));
    }
}
