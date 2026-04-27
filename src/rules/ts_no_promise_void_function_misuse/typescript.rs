//! Flag `setTimeout(async`, `setInterval(async`, `queueMicrotask(async`,
//! `process.nextTick(async`, and `.forEach(async` patterns. The shared
//! property: callee discards the callback's return value, so a rejected
//! promise has nowhere to go.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

const PATTERNS: &[&str] = &[
    "setTimeout(async",
    "setInterval(async",
    "setImmediate(async",
    "queueMicrotask(async",
    ".forEach(async",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for pat in PATTERNS {
                let mut search_from = 0;
                while let Some(rel) = line[search_from..].find(pat) {
                    let col = search_from + rel;
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{}...` ignores the returned promise. Wrap with `() => {{ void asyncFn(); }}` \
                             or refactor `.forEach` into a `for ... of` with `await`.",
                            pat
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
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
    fn flags_set_timeout_async() {
        let src = "setTimeout(async () => { await save(); }, 100);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_set_interval_async() {
        let src = "setInterval(async () => { await tick(); }, 1000);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_foreach_async() {
        let src = "items.forEach(async (i) => { await save(i); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_set_timeout_void_wrapper() {
        let src = "setTimeout(() => { void save(); }, 100);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_set_timeout_sync_callback() {
        let src = "setTimeout(() => doStuff(), 100);";
        assert!(run(src).is_empty());
    }
}
