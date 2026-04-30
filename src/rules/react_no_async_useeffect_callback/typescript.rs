//! Flag `useEffect(async` patterns. Detection is text-based — match the
//! literal substring at a word boundary.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

const NEEDLES: &[&str] = &[
    "useEffect(async",
    "useLayoutEffect(async",
    "useInsertionEffect(async",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for n in NEEDLES {
                let mut from = 0;
                while let Some(rel) = line[from..].find(n) {
                    let col = from + rel;
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{n}...` returns a promise — React expects a sync callback whose return is the \
                             cleanup. Define an async function inside and call it instead."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                    from = col + n.len();
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
        Check.check(&CheckCtx::for_test(Path::new("c.tsx"), source))
    }

    #[test]
    fn flags_async_useeffect() {
        let src = "useEffect(async () => { await fetch('/x'); }, []);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_async_uselayouteffect() {
        let src = "useLayoutEffect(async () => { await fn(); }, []);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_sync_useeffect_with_inner_async() {
        let src = "useEffect(() => { (async () => { await fetch('/x'); })(); }, []);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_sync_useeffect() {
        let src = "useEffect(() => { console.log('hi'); }, []);";
        assert!(run(src).is_empty());
    }
}
