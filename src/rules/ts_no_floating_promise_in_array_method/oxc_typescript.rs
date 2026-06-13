use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const PATTERNS: &[&str] = &[".forEach(async", ".map(async", ".filter(async"];

fn line_already_uses_promise_all(line: &str, match_col: usize) -> bool {
    let mut end = match_col.min(line.len());
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = &line[..end];
    prefix.contains("Promise.all(") || prefix.contains("Promise.allSettled(")
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(PATTERNS)
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for pat in PATTERNS {
                let mut search_from = 0;
                while let Some(rel) = line[search_from..].find(pat) {
                    let col = search_from + rel;
                    if !line_already_uses_promise_all(line, col) {
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
}
