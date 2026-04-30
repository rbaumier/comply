use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // Build a lowercase map of column types: column_name -> type. We only
        // need a rough scan — find `<ident> BOOLEAN` patterns within
        // CREATE TABLE blocks.
        let lower = ctx.source.to_ascii_lowercase();
        let bool_columns = collect_boolean_columns(&lower);

        // Now walk CREATE INDEX statements and flag any that index a boolean
        // column without a WHERE clause.
        for (idx, line) in ctx.source.lines().enumerate() {
            let lower_line = line.to_ascii_lowercase();
            if !lower_line.contains("create index") && !lower_line.contains("create unique index") {
                continue;
            }
            // Has a WHERE clause? Then it's a partial index — fine.
            if lower_line.contains(" where ") {
                continue;
            }
            // Extract the column list inside parentheses.
            let Some(open) = lower_line.find('(') else {
                continue;
            };
            let Some(close) = lower_line[open + 1..].find(')') else {
                continue;
            };
            let col_list = &lower_line[open + 1..open + 1 + close];
            let cols: Vec<&str> = col_list.split(',').map(|c| c.trim()).collect();
            if cols.iter().any(|c| bool_columns.contains(*c)) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-index-on-low-cardinality-boolean".into(),
                    message: "B-tree index on a boolean column has too low selectivity to be useful. Use a partial index (`WHERE flag = TRUE`) or drop it.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

/// Scan lowered source for `<ident> boolean` (or `bool`) declarations.
/// Returns a set of column names known to be boolean.
fn collect_boolean_columns(lower: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for line in lower.lines() {
        // Skip CREATE INDEX lines — only column declarations.
        if line.contains("create index") {
            continue;
        }
        // Find `boolean` or `bool` as a whole word.
        for kw in &["boolean", "bool"] {
            let bytes = line.as_bytes();
            let needle = kw.as_bytes();
            let mut i = 0;
            while i + needle.len() <= bytes.len() {
                if bytes[i..i + needle.len()] == *needle {
                    let before_ok = i == 0 || !is_ident(bytes[i - 1]);
                    let after_idx = i + needle.len();
                    let after_ok = after_idx >= bytes.len() || !is_ident(bytes[after_idx]);
                    if before_ok && after_ok {
                        // Walk backwards from `i` to find the column identifier.
                        if let Some(name) = preceding_ident(line, i) {
                            out.insert(name);
                        }
                    }
                }
                i += 1;
            }
        }
    }
    out
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn preceding_ident(line: &str, type_pos: usize) -> Option<String> {
    let bytes = line.as_bytes();
    // Skip whitespace right before `type_pos`.
    let mut end = type_pos;
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    let mut start = end;
    while start > 0 && is_ident(bytes[start - 1]) {
        start -= 1;
    }
    if start == end {
        return None;
    }
    Some(line[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.sql"), src))
    }

    #[test]
    fn flags_index_on_boolean_column() {
        let src = "CREATE TABLE users (id INT, is_active BOOLEAN);\nCREATE INDEX idx ON users(is_active);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_partial_index_on_boolean() {
        let src = "CREATE TABLE users (id INT, is_active BOOLEAN);\nCREATE INDEX idx ON users(is_active) WHERE is_active = TRUE;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_index_on_non_boolean() {
        let src = "CREATE TABLE users (id INT, email TEXT);\nCREATE INDEX idx ON users(email);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_index_on_bool_short_form() {
        let src = "CREATE TABLE u (flag BOOL);\nCREATE INDEX idx ON u(flag);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_lowercase() {
        let src = "create table u (id int, is_active boolean);\ncreate index idx on u(is_active);";
        assert_eq!(run(src).len(), 1);
    }
}
