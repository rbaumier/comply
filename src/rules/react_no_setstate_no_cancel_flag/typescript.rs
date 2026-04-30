//! Heuristic detection: scan each `useEffect(` block; if the body contains
//! both `await ` and a `setX(` call but no cancellation marker
//! (`cancelled`, `cancelled = true`, `AbortController`, `signal`), flag it.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

const CANCEL_MARKERS: &[&str] = &[
    "cancelled",
    "cancel",
    "AbortController",
    ".signal",
    "abort",
    "isMounted",
    "mounted",
];

/// Find the matching `)` for the `(` at index `start`, returning the byte
/// index of the matching paren (or None if unbalanced before EOF).
fn find_matching_paren(bytes: &[u8], start: usize) -> Option<usize> {
    debug_assert_eq!(bytes[start], b'(');
    let mut depth: i32 = 0;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn body_uses_set_state(body: &str) -> bool {
    // Cheap: any identifier matching `set[A-Z]` followed by `(`.
    let bytes = body.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if &bytes[i..i + 3] == b"set"
            && bytes[i + 3].is_ascii_uppercase()
            && (i == 0 || (!bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_'))
        {
            // Find next non-ident char and check it's `(`.
            let mut j = i + 3;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'(' {
                return true;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    false
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useEffect"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let bytes = ctx.source.as_bytes();
        let mut search_from = 0;
        while let Some(rel) = ctx.source[search_from..].find("useEffect(") {
            let abs = search_from + rel;
            let paren_open = abs + "useEffect".len();
            let Some(paren_close) = find_matching_paren(bytes, paren_open) else {
                break;
            };
            let body = &ctx.source[paren_open + 1..paren_close];
            if body.contains("await ")
                && body_uses_set_state(body)
                && !CANCEL_MARKERS.iter().any(|m| body.contains(m))
            {
                // Compute (line, col) for `useEffect`.
                let prefix = &ctx.source[..abs];
                let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
                let col = prefix.rfind('\n').map_or(abs, |nl| abs - nl - 1) + 1;
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column: col,
                    rule_id: super::META.id.into(),
                    message: "Async `useEffect` calls `setState` after `await` without a cancellation \
                              flag — when the effect re-runs or the component unmounts, you'll see \
                              \"state update on unmounted component\". Track a `cancelled` flag and skip \
                              the setter when set."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            search_from = paren_close + 1;
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
    fn flags_useeffect_setstate_no_flag() {
        let src = "useEffect(() => { (async () => { const r = await fetch('/'); setData(r); })(); }, []);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_cancelled_flag() {
        let src = "useEffect(() => { let cancelled = false; (async () => { const r = await fetch('/'); if (!cancelled) setData(r); })(); return () => { cancelled = true; }; }, []);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_with_abort_controller() {
        let src = "useEffect(() => { const ac = new AbortController(); (async () => { const r = await fetch('/', { signal: ac.signal }); setData(r); })(); return () => ac.abort(); }, []);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_useeffect_no_await() {
        let src = "useEffect(() => { setData(42); }, []);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_await_no_setstate() {
        let src = "useEffect(() => { (async () => { await sendMetric(); })(); }, []);";
        assert!(run(src).is_empty());
    }
}
