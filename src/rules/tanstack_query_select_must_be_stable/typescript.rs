//! Flag `select:` keys whose value is an inline arrow function not
//! wrapped with `useCallback`. We only flag obvious arrows — the value
//! starts with `(` (paren-arg arrow) or an identifier followed by `=>`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn byte_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Look for `select:` followed by whitespace and then an inline arrow
/// expression (not `useCallback`).
fn find_offenses(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = source[from..].find("select:") {
        let key_start = from + rel;
        let after = key_start + "select:".len();
        // Skip whitespace.
        let mut i = after;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let rest = &source[i..];
        // Allowlist: `useCallback(` or a bare identifier reference (e.g.
        // `select: stableSelector`) — both are stable references.
        if rest.starts_with("useCallback(") {
            from = i;
            continue;
        }
        // Detect arrow: `(args) =>` or `arg =>` or `async (args) =>`.
        let trimmed = rest.trim_start_matches("async ").trim_start();
        let is_arrow = if let Some(rest_after_paren) = trimmed.strip_prefix('(') {
            // Find matching `)`.
            let bs = rest_after_paren.as_bytes();
            let mut depth = 1i32;
            let mut j = 0;
            while j < bs.len() && depth > 0 {
                match bs[j] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            if depth == 0 {
                let after_paren = rest_after_paren[j..].trim_start();
                after_paren.starts_with("=>")
                    || after_paren.starts_with(": ")
                    || after_paren.starts_with(":")
            } else {
                false
            }
        } else {
            // `arg =>` — single identifier.
            let mut k = 0;
            let tb = trimmed.as_bytes();
            while k < tb.len()
                && (tb[k].is_ascii_alphanumeric() || tb[k] == b'_' || tb[k] == b'$')
            {
                k += 1;
            }
            if k > 0 {
                trimmed[k..].trim_start().starts_with("=>")
            } else {
                false
            }
        };
        if is_arrow {
            out.push(key_start);
        }
        from = i;
    }
    out
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Only consider files that look like they use TanStack Query.
        if !ctx.source.contains("useQuery") && !ctx.source.contains("queryOptions") {
            return Vec::new();
        }
        find_offenses(ctx.source)
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`select:` is an inline arrow — wrap with `useCallback` or hoist to module scope so the selector reference is stable."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), source))
    }

    #[test]
    fn flags_inline_arrow_select() {
        let src = "useQuery({ queryKey: ['k'], select: (data) => data.foo })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_single_arg_arrow() {
        let src = "useQuery({ queryKey: ['k'], select: data => data.foo })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_use_callback_select() {
        let src = "useQuery({ queryKey: ['k'], select: useCallback((data) => data.foo, []) })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_named_reference_select() {
        let src = "useQuery({ queryKey: ['k'], select: stableSelector })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_query_files() {
        let src = "const x = { select: (d) => d.foo };";
        assert!(run(src).is_empty());
    }
}
