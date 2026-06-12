//! Flag `.forEach(async`, `.map(async`, `.flatMap(async` and `.filter(async`
//! patterns. When the call is an argument of `Promise.all(`,
//! `Promise.allSettled(` or `Promise.race(` we accept it — the caller is
//! awaiting completion. Detection is text-based: from the match we scan the
//! preceding source for the nearest enclosing open `(` (across line breaks)
//! and exempt the match when that paren belongs to one of those combinators.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

const PATTERNS: &[&str] = &[
    ".forEach(async",
    ".map(async",
    ".flatMap(async",
    ".filter(async",
];

const AWAITING_COMBINATORS: &[&str] =
    &["Promise.all", "Promise.allSettled", "Promise.race"];

/// True when the byte at `match_offset` in `source` sits inside the argument
/// list of `Promise.all(`, `Promise.allSettled(` or `Promise.race(`. Scans
/// backward tracking `(`/`)` depth; the first unmatched `(` is the enclosing
/// call's paren, whose preceding identifier decides the verdict.
fn enclosed_in_awaiting_combinator(source: &str, match_offset: usize) -> bool {
    let prefix = &source.as_bytes()[..match_offset];
    let mut depth: i32 = 0;
    for (i, &b) in prefix.iter().enumerate().rev() {
        match b {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    let before = &source[..i];
                    return AWAITING_COMBINATORS.iter().any(|c| before.ends_with(c));
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    false
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(PATTERNS)
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let base = ctx.source.as_ptr() as usize;
        for (idx, line) in ctx.source.lines().enumerate() {
            // `line` is a slice of `ctx.source`; its byte offset is exact for
            // any line ending, so backward scans align with the full source.
            let line_start = line.as_ptr() as usize - base;
            for pat in PATTERNS {
                let mut search_from = 0;
                while let Some(rel) = line[search_from..].find(pat) {
                    let col = search_from + rel;
                    if !enclosed_in_awaiting_combinator(ctx.source, line_start + col) {
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line: idx + 1,
                            column: col + 1,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{}` floats promises — the iteration finishes before the async \
                                 work does. Use a `for ... of` loop with `await`, or wrap with \
                                 `await Promise.all(arr.map(async ...))`.",
                                pat.trim_start_matches('.')
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    search_from = col + pat.len();
                }
            }
        }
        diagnostics
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
    fn flags_foreach_async() {
        let src = "items.forEach(async (x) => { await save(x); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_map_async() {
        let src = "const r = arr.map(async (x) => fetchOne(x));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_promise_all_map_async() {
        let src = "const r = await Promise.all(arr.map(async (x) => fetchOne(x)));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_of_loop() {
        let src = "for (const x of arr) { await fn(x); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_filter_async() {
        let src = "const f = arr.filter(async (x) => isReady(x));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_promise_all_map_async_across_lines() {
        // Regression for #1095: the `Promise.all(` opening and the
        // `.map(async` sit on different lines.
        let src = "initTasks.push(\n\
                   \x20\x20Promise.all(\n\
                   \x20\x20\x20\x20parsedConfigs.map(async (pc) => {\n\
                   \x20\x20\x20\x20\x20\x20await discoverPolyfills(pc);\n\
                   \x20\x20\x20\x20}),\n\
                   \x20\x20).then(() => {}),\n\
                   );";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_promise_race_map_async() {
        let src = "const r = await Promise.race(arr.map(async (x) => fetchOne(x)));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_flat_map_async() {
        let src = "const r = arr.flatMap(async (x) => fetchMany(x));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_map_async_wrapped_in_non_combinator_call() {
        // Still floats: the enclosing call is not an awaiting combinator.
        let src = "void doStuff(\n  arr.map(async (x) => fetchOne(x)),\n);";
        assert_eq!(run(src).len(), 1);
    }
}
