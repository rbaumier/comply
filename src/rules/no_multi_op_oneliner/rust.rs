//! no-multi-op-oneliner backend for Rust.
//!
//! Flags single lines with 6+ operator-like tokens crammed together.
//! Same heuristic as the TypeScript version — favors false negatives
//! over false positives.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const DEFAULT_MIN_OPS: usize = 6;
const DEFAULT_MIN_LINE_LENGTH: usize = 80;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let min_ops = ctx
            .config
            .threshold("no-multi-op-oneliner", "min_ops", DEFAULT_MIN_OPS);
        let min_line_length = ctx.config.threshold(
            "no-multi-op-oneliner",
            "min_line_length",
            DEFAULT_MIN_LINE_LENGTH,
        );
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut reported_lines = std::collections::HashSet::new();
        let mut diagnostics = Vec::new();

        walk_tree(tree, |node| {
            if node.kind() != "let_declaration" && node.kind() != "expression_statement" {
                return;
            }
            let start = node.start_position();
            let end = node.end_position();
            if start.row != end.row {
                return;
            }
            if reported_lines.contains(&start.row) {
                return;
            }
            let Some(line) = lines.get(start.row) else {
                return;
            };
            if line.len() < min_line_length {
                return;
            }
            let ops = count_operators(line);
            if ops < min_ops {
                return;
            }
            reported_lines.insert(start.row);
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: start.row + 1,
                column: 1,
                rule_id: "no-multi-op-oneliner".into(),
                message: format!(
                    "Line has {ops} chained operations — extract intermediate \
                     named `let` bindings so each step's purpose is visible."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

fn count_operators(line: &str) -> usize {
    let mut count = 0;
    let mut in_string = false;
    let mut prev: u8 = 0;
    for &b in line.as_bytes() {
        if (b == b'"' || b == b'\'') && prev != b'\\' {
            in_string = !in_string;
            prev = b;
            continue;
        }
        if in_string {
            prev = b;
            continue;
        }
        if matches!(b, b'.' | b'+' | b'-' | b'*' | b'/' | b'%' | b'(') {
            count += 1;
        }
        prev = b;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.rs"), source),
            &tree,
        )
    }

    #[test]
    fn flags_heavy_oneliner() {
        let source = "fn f() { let total = items.iter().filter(|i| i.active).map(|i| i.price).sum::<f64>() * tax + discount; }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_simple_oneliner() {
        assert!(run_on("fn f() { let x = a + b; }").is_empty());
    }
}
