//! Shared dense-line detection used by both the Rust and TypeScript
//! backends. The only thing that differs between the two is which
//! AST node kinds to scan as candidates — Rust looks at
//! `let_declaration` / `expression_statement`, TypeScript looks at
//! `expression_statement` / `variable_declarator`. Everything else
//! (threshold reading, line-length floor, operator counting,
//! diagnostic message) lives here.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::CheckCtx;
use crate::rules::walker::walk_tree;

const DEFAULT_MIN_OPS: usize = 6;
const DEFAULT_MIN_LINE_LENGTH: usize = 80;

/// Scan `tree` for single-line statements whose source line crosses both
/// the `min_line_length` and `min_ops` thresholds. `target_kinds` is the
/// set of tree-sitter node kinds to consider as candidates (varies by
/// language).
#[must_use]
pub fn scan_dense_lines(
    ctx: &CheckCtx,
    tree: &tree_sitter::Tree,
    target_kinds: &[&str],
) -> Vec<Diagnostic> {
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
        if !target_kinds.contains(&node.kind()) {
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
                 named bindings so each step's purpose is visible."
            ),
            severity: Severity::Warning,
        });
    });
    diagnostics
}

/// Count operator-like tokens on a line: call parens, binary operators,
/// member-access dots, spread, and comparison operators. Skips string
/// literals (single, double, backtick) so punctuation inside text isn't
/// counted.
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
