//! Shared dense-line detection used by both the Rust and TypeScript
//! backends.
//!
//! ## Why operator counting needs comment stripping
//!
//! The naive byte-scan version of this rule (count `.`, `+`, `-`, `*`,
//! `/`, `%`, `(` outside of string literals) misfired on lines with
//! trailing `// comment` blocks: every `/`, `-`, and `.` inside the
//! comment counted as an operator, so a perfectly idiomatic test
//! line like:
//!
//! ```ignore
//! assert_eq!(run("utils.spec.ts", "// TODO: add tests").len(), 1); // comply-ignore: todo-needs-issue-link — test content, not a real marker.
//! ```
//!
//! reported "11 chained operations" — 4 real ops in the code plus 7
//! noise tokens (`//`, four hyphens in `todo-needs-issue-link`, the
//! trailing `.`) inside the comment.
//!
//! ## Fix: use tree-sitter comment node ranges
//!
//! Instead of trying to re-discover comments from raw bytes, we ask
//! tree-sitter directly. Both the TS and Rust grammars expose comment
//! nodes; we collect their byte ranges once per file, then for each
//! candidate line we compute a "stripped line" that has every byte
//! falling inside any comment range removed. The stripped line is
//! used for BOTH the operator count and the `min_line_length` check —
//! a 30-char code line followed by a 90-char comment shouldn't trip
//! the rule.
//!
//! Block comments (`/* … */`) work the same way: the grammar reports
//! their full byte range, which can span lines, and the stripper
//! handles each line's overlap with that range independently.
//!
//! Comment node kinds vary by language (`comment` in TS, `line_comment`
//! plus `block_comment` in Rust), so callers pass the kinds they care
//! about as a slice parameter.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::CheckCtx;
use crate::rules::walker::walk_tree;

/// Scan `tree` for single-line statements whose stripped source line
/// crosses both the `min_line_length` and `min_ops` thresholds.
///
/// `target_kinds` is the set of tree-sitter node kinds to consider as
/// candidate statements (`expression_statement` / `variable_declarator`
/// for TS, `let_declaration` / `expression_statement` for Rust).
///
/// `comment_kinds` is the set of node kinds the grammar uses for
/// comments (`comment` for TS, `line_comment` + `block_comment` for
/// Rust). Their byte ranges are stripped from candidate lines before
/// the operator count and length check.
#[must_use]
pub fn scan_dense_lines(
    ctx: &CheckCtx,
    tree: &tree_sitter::Tree,
    target_kinds: &[&str],
    comment_kinds: &[&str],
) -> Vec<Diagnostic> {
    let min_ops = ctx.config.threshold("no-multi-op-oneliner", "min_ops");
    let min_line_length = ctx
        .config
        .threshold("no-multi-op-oneliner", "min_line_length");

    let line_offsets = compute_line_offsets(ctx.source);
    let comment_ranges = collect_comment_ranges(tree, comment_kinds);

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
        let Some(&(line_start_byte, line_text)) = line_offsets.get(start.row) else {
            return;
        };
        let stripped = strip_comments(line_text, line_start_byte, &comment_ranges);
        if stripped.len() < min_line_length {
            return;
        }
        let ops = count_operators(&stripped);
        if ops < min_ops {
            return;
        }
        reported_lines.insert(start.row);
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: start.row + 1,
            column: 1,
            rule_id: "no-multi-op-oneliner".into(),
            message: format!(
                "Line has {ops} chained operations — extract intermediate \
                 named bindings so each step's purpose is visible."
            ),
            severity: Severity::Warning,
            span: None,
        });
    });
    diagnostics
}

/// Walk the file once and split it into `(line_start_byte, line_text)`
/// tuples. `line_text` excludes the trailing `\n` / `\r\n` so that
/// `line_text.len()` matches what we want to compare against
/// `min_line_length`. The byte offsets are absolute in the source.
fn compute_line_offsets(source: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut byte_offset = 0;
    for chunk in source.split_inclusive('\n') {
        let trimmed = chunk.trim_end_matches(['\n', '\r'].as_ref());
        out.push((byte_offset, trimmed));
        byte_offset += chunk.len();
    }
    out
}

/// Collect `(start_byte, end_byte)` ranges for every comment node in
/// the tree. Delegates the cursor walk to `walker::collect_nodes_of_kinds`
/// and maps each node to its byte range.
fn collect_comment_ranges(tree: &tree_sitter::Tree, comment_kinds: &[&str]) -> Vec<(usize, usize)> {
    crate::rules::walker::collect_nodes_of_kinds(tree, comment_kinds)
        .into_iter()
        .map(|n| (n.start_byte(), n.end_byte()))
        .collect()
}

/// Return a copy of `line_text` with every byte that falls inside any
/// `(start_byte, end_byte)` comment range removed. Block comments that
/// straddle the line are clamped to the line's bounds.
///
/// `line_start_byte` is the absolute byte offset where `line_text`
/// begins in the original source.
fn strip_comments(line_text: &str, line_start_byte: usize, ranges: &[(usize, usize)]) -> String {
    if ranges.is_empty() {
        return line_text.to_string();
    }
    let line_end_byte = line_start_byte + line_text.len();
    let line_bytes = line_text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(line_text.len());
    let mut local: usize = 0;
    while local < line_bytes.len() {
        let abs = line_start_byte + local;
        let in_range = ranges.iter().find(|(s, e)| abs >= *s && abs < *e);
        if let Some((_, e)) = in_range {
            // Skip until the end of this comment, clamped to end of line.
            let new_abs = (*e).min(line_end_byte);
            local += new_abs - abs;
        } else {
            out.push(line_bytes[local]);
            local += 1;
        }
    }
    // Bytes copied from a UTF-8 string at byte boundaries that don't
    // cut across a comment range remain valid UTF-8.
    String::from_utf8(out).unwrap_or_default()
}

/// Count operator-like tokens on a line: call parens, binary
/// operators, member-access dots. Skips string literals so
/// punctuation inside text isn't counted.
///
/// Comment stripping happens before this is called; nothing in the
/// input string should be a comment.
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

    #[test]
    fn line_offsets_align_with_source_bytes() {
        let src = "alpha\nbeta\r\ngamma";
        let infos = compute_line_offsets(src);
        assert_eq!(infos.len(), 3);
        assert_eq!(infos[0], (0, "alpha"));
        assert_eq!(infos[1], (6, "beta"));
        assert_eq!(infos[2], (12, "gamma"));
    }

    #[test]
    fn strip_comments_removes_byte_range() {
        // line `code // foo` with the range covering `// foo`.
        let line = "code // foo";
        // `//` starts at byte 5 in this line; line_start_byte is 0.
        let stripped = strip_comments(line, 0, &[(5, line.len())]);
        assert_eq!(stripped, "code ");
    }

    #[test]
    fn strip_comments_preserves_line_when_no_range_overlaps() {
        let line = "code without comments";
        let stripped = strip_comments(line, 100, &[(0, 50), (200, 300)]);
        assert_eq!(stripped, line);
    }

    #[test]
    fn count_operators_on_clean_code() {
        assert_eq!(count_operators("a.b.c.d()"), 4);
    }
}
