//! sql-index-needs-rationale-comment — Rust backend.
//!
//! Walks every `string_literal` / `raw_string_literal` node in the Rust
//! AST, scans their content for `CREATE INDEX` / `CREATE UNIQUE INDEX`,
//! and emits a diagnostic when no nearby `-- ...` SQL comment justifies
//! the index.
//!
//! Exposes `check_string_content` — the pure-text detection routine —
//! for reuse by the TypeScript backend, which applies the same logic to
//! `string` / `template_string` nodes. Keeps the regex compilation and
//! the "≥3 lines back" heuristic in one place.

use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["string_literal", "raw_string_literal"];

fn create_index_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\bCREATE\s+(?:UNIQUE\s+)?INDEX\b").expect("static regex compiles")
    })
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let min_rationale_words = ctx
            .config
            .threshold("sql-index-needs-rationale-comment", "min_rationale_words", ctx.lang);
        let lookback_lines = ctx
            .config
            .threshold("sql-index-needs-rationale-comment", "lookback_lines", ctx.lang);
        let source_bytes = ctx.source.as_bytes();
        let Ok(raw) = node.utf8_text(source_bytes) else {
            return;
        };
        let content = strip_rust_string_delimiters(raw);
        let start = node.start_position();
        // Content offset inside the literal: advance past the opening
        // delimiter so diagnostic columns land inside the SQL, not on
        // the leading quote.
        let delimiter_len = raw.len().saturating_sub(content.len()).saturating_sub(
            // trailing quote length matches leading quote length
            raw.len().saturating_sub(content.len()) / 2,
        );
        diagnostics.extend(check_string_content(
            content,
            start.row,
            start.column + delimiter_len,
            ctx.path,
            min_rationale_words,
            lookback_lines,
        ));
    }
}

/// Strip Rust string-literal delimiters so the inner content can be
/// scanned line-by-line. Handles `"..."`, `r"..."`, `r#"..."#`,
/// `b"..."`, `br"..."`, `br#"..."#`, and `c"..."` prefixes.
fn strip_rust_string_delimiters(raw: &str) -> &str {
    // Strip leading b / br / r / c prefix bytes.
    let after_prefix = raw.trim_start_matches(['b', 'r', 'c']);
    // Strip any number of `#` characters on each side (raw strings).
    let hashes = after_prefix.bytes().take_while(|b| *b == b'#').count();
    let after_hashes = &after_prefix[hashes..];
    // Expect a leading quote. If missing, we can't safely strip — return raw.
    let Some(inner) = after_hashes.strip_prefix('"') else {
        return raw;
    };
    // Trim matching trailing `"#*`.
    let trailing_len = 1 + hashes;
    if inner.len() < trailing_len {
        return inner;
    }
    &inner[..inner.len() - trailing_len]
}

/// Scan `content` for `CREATE INDEX` occurrences and return a diagnostic
/// for each one that lacks a nearby `-- ...` SQL rationale comment.
///
/// - `node_start_line` / `node_start_col`: the 0-based start position of
///   the string literal node in the source file. Diagnostic line/column
///   are computed relative to these so the squiggle lands on the actual
///   `CREATE INDEX` inside the literal, not on the opening quote.
/// - `path`: the file being linted, forwarded into each `Diagnostic`.
/// - `min_rationale_words`: threshold for how many non-trivial tokens
///   a `-- ...` comment must carry to count as a real rationale.
/// - `lookback_lines`: how many lines before the `CREATE INDEX` line
///   are scanned for an explanatory comment.
pub(super) fn check_string_content(
    content: &str,
    node_start_line: usize,
    node_start_col: usize,
    path: &Path,
    min_rationale_words: usize,
    lookback_lines: usize,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = content.split('\n').collect();
    let re = create_index_re();

    for (idx, line) in lines.iter().enumerate() {
        let upper = line.to_ascii_uppercase();
        let Some(m) = re.find(&upper) else {
            continue;
        };
        if has_rationale_before(&lines, idx, lookback_lines, min_rationale_words)
            || has_trailing_rationale(line, m.start(), min_rationale_words)
        {
            continue;
        }
        // Compute position: first line of the literal keeps the node's
        // column offset (so diagnostic points past the opening quote),
        // subsequent lines start at column 1 of that line.
        let diag_line = node_start_line + idx + 1; // 1-based
        let diag_col = if idx == 0 {
            node_start_col + m.start() + 1
        } else {
            m.start() + 1
        };
        diagnostics.push(Diagnostic {
            path: path.to_path_buf().into(),
            line: diag_line,
            column: diag_col,
            rule_id: "sql-index-needs-rationale-comment".into(),
            message: "`CREATE INDEX` without a rationale comment. Every index costs \
                      write throughput and disk — explain what query it accelerates \
                      so future readers know whether it's still useful."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
    diagnostics
}

/// True if any of the `lookback_lines` lines before `idx` contains a
/// `-- ...` comment with at least `min_rationale_words` non-trivial
/// tokens.
fn has_rationale_before(
    lines: &[&str],
    idx: usize,
    lookback_lines: usize,
    min_rationale_words: usize,
) -> bool {
    let start = idx.saturating_sub(lookback_lines);
    lines[start..idx].iter().any(|line| {
        line.find("--")
            .is_some_and(|pos| is_real_rationale(&line[pos + 2..], min_rationale_words))
    })
}

/// True if `line` has a trailing `-- ...` comment (after the
/// `CREATE INDEX` match) with at least `min_rationale_words`
/// non-trivial tokens.
fn has_trailing_rationale(
    line: &str,
    create_index_start: usize,
    min_rationale_words: usize,
) -> bool {
    line[create_index_start..].find("--").is_some_and(|pos| {
        is_real_rationale(&line[create_index_start + pos + 2..], min_rationale_words)
    })
}

/// A `-- ...` comment counts as a rationale when it contains at least
/// `min_rationale_words` whitespace-separated tokens of length >1. Rejects
/// drive-by notes like `-- ok`, `-- TODO`, or empty `--`.
fn is_real_rationale(comment_body: &str, min_rationale_words: usize) -> bool {
    comment_body
        .split_whitespace()
        .filter(|w| w.chars().count() > 1)
        .count()
        >= min_rationale_words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_in_rust_raw_string() {
        let src = r#"fn f() { sqlx::query!(r"CREATE INDEX idx_x ON t(c);"); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_in_rust_with_comment() {
        let src = "fn f() { \
                   sqlx::query!(\"-- explains why index exists for dashboard\\nCREATE INDEX idx_x ON t(c);\"); \
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_rust_non_sql_string() {
        assert!(run_on(r#"fn f() { let s = "hello world"; }"#).is_empty());
    }

    #[test]
    fn flags_rust_create_unique_index() {
        let src = r#"fn f() { let q = "CREATE UNIQUE INDEX idx_x ON t(c);"; }"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
