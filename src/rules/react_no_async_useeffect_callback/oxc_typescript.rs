use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const NEEDLES: &[&str] = &[
    "useEffect(async",
    "useLayoutEffect(async",
    "useInsertionEffect(async",
];

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(NEEDLES)
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
