//! For each `dehydrate(` occurrence, scan the preceding ~4KB for any
//! `prefetchQuery(` not preceded by `await`. If found, flag the
//! dehydrate call site.

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

const WINDOW: usize = 4096;

/// True when `prefetchQuery(` at offset `abs` (inside `window`) is the
/// tail of an awaited expression. We walk back across receiver chains
/// (`x.y.prefetchQuery(`), past optional `?`, then check for `await`.
fn is_awaited_at(window: &str, abs: usize) -> bool {
    let bytes = window.as_bytes();
    let mut i = abs;
    // Skip back over `<id>.` or `<id>?.` chain.
    loop {
        // Skip identifier chars.
        while i > 0 {
            let c = bytes[i - 1];
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' {
                i -= 1;
            } else {
                break;
            }
        }
        // Optional `.` or `?.`.
        if i >= 1 && bytes[i - 1] == b'.' {
            i -= 1;
            if i >= 1 && bytes[i - 1] == b'?' {
                i -= 1;
            }
            continue;
        }
        break;
    }
    // Now i points at the start of the receiver. Check if `await ` precedes.
    let prefix = &window[..i];
    prefix.trim_end().ends_with("await")
}

fn has_unawaited_prefetch_before(source: &str, dehydrate_offset: usize) -> bool {
    let mut start = dehydrate_offset.saturating_sub(WINDOW);
    while start > 0 && !source.is_char_boundary(start) {
        start -= 1;
    }
    let window = &source[start..dehydrate_offset];
    let mut from = 0usize;
    while let Some(rel) = window[from..].find("prefetchQuery(") {
        let abs = from + rel;
        if is_awaited_at(window, abs) {
            from = abs + 1;
            continue;
        }
        // Inside `await Promise.all([..., x.prefetchQuery(...), ...])` — fine.
        // Heuristic: look for `await Promise.all(` somewhere before `abs`.
        // This is an over-approximation but matches typical SSR setup code.
        if window[..abs].contains("await Promise.all(") {
            from = abs + 1;
            continue;
        }
        return true;
    }
    false
}

fn find_offenses(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = source[from..].find("dehydrate(") {
        let abs = from + rel;
        // Word boundary check.
        let pre = source.as_bytes().get(abs.saturating_sub(1)).copied();
        let is_boundary = pre.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'$');
        if is_boundary && has_unawaited_prefetch_before(source, abs) {
            out.push(abs);
        }
        from = abs + 1;
    }
    out
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["dehydrate"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source_contains("dehydrate(") || !ctx.source_contains("prefetchQuery(") {
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
                    message: "`dehydrate(...)` runs before an `await prefetchQuery(...)` — \
                              pending queries serialize empty."
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_unawaited_prefetch() {
        let src =
            "queryClient.prefetchQuery({ queryKey: ['x'] }); const state = dehydrate(queryClient);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_awaited_prefetch() {
        let src = "await queryClient.prefetchQuery({ queryKey: ['x'] }); const state = dehydrate(queryClient);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_when_no_dehydrate() {
        let src = "queryClient.prefetchQuery({ queryKey: ['x'] });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_promise_all_awaited() {
        let src = "await Promise.all([queryClient.prefetchQuery({ queryKey: ['x'] }), queryClient.prefetchQuery({ queryKey: ['y'] })]); const state = dehydrate(queryClient);";
        assert!(run(src).is_empty());
    }
}
