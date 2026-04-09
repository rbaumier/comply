//! no-multi-op-oneliner backend — reject single lines with 4+ chained
//! operators crammed together.
//!
//! Why: `const x = items.filter(i => i.active).map(i => i.price).reduce((a,b) => a+b, 0) * tax + discount;`
//! is unreadable. Extract intermediate named variables — `active`,
//! `prices`, `subtotal`, `total` — so each step's purpose is visible.
//!
//! Detection: for every `expression_statement` / `variable_declarator`
//! that spans a single line, count the operator-like tokens on that line
//! (call parens, binary operators, member access dots). Flag when the
//! count crosses the threshold.
//!
//! This is a heuristic. It deliberately prefers false negatives over
//! false positives: mundane one-liners don't trip it.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const MIN_OPS: usize = 6;
const MIN_LINE_LENGTH: usize = 80;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut reported_lines = std::collections::HashSet::new();
        let mut diagnostics = Vec::new();

        walk_tree(tree, |node| {
            if node.kind() != "expression_statement" && node.kind() != "variable_declarator" {
                return;
            }
            let start = node.start_position();
            let end = node.end_position();
            if start.row != end.row {
                return; // multi-line, probably already formatted well
            }
            if reported_lines.contains(&start.row) {
                return;
            }
            let Some(line) = lines.get(start.row) else {
                return;
            };
            if line.len() < MIN_LINE_LENGTH {
                return;
            }
            let ops = count_operators(line);
            if ops < MIN_OPS {
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
                     named variables so each step's purpose is visible."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

/// Count operator-like tokens on a line: call parens, binary operators,
/// member-access dots, spread, and comparison operators. Skips string
/// literals to avoid counting punctuation inside user-facing text.
fn count_operators(line: &str) -> usize {
    let mut count = 0;
    let mut in_string = false;
    let mut prev: u8 = 0;
    for &b in line.as_bytes() {
        if (b == b'"' || b == b'\'' || b == b'`') && prev != b'\\' {
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
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx {
                path: Path::new("t.ts"),
                source,
            },
            &tree,
        )
    }

    #[test]
    fn flags_heavy_oneliner() {
        let source = "const total = items.filter(i => i.active).map(i => i.price).reduce((a, b) => a + b, 0) * tax + discount;";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_simple_oneliner() {
        assert!(run_on("const x = a + b;").is_empty());
    }

    #[test]
    fn allows_short_but_dense_expression() {
        // Dense but short — under the line-length floor.
        assert!(run_on("const x = a.b.c + d.e * f;").is_empty());
    }
}
