//! For each `useEffect(...)` call, parse the body. If it contains
//! `router.push(` (or `.replace(`/`.back(`/`.forward(`) AND the deps
//! array is empty `[]`, flag.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

const ROUTER_METHODS: &[&str] = &[
    "router.push(",
    "router.replace(",
    ".push(",
    ".replace(",
];

fn find_matching_paren(bytes: &[u8], start: usize) -> Option<usize> {
    debug_assert_eq!(bytes[start], b'(');
    let mut depth: i32 = 0;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 { return Some(i); }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Find the deps array inside a `useEffect(...)` body — the LAST
/// `[...]` block in the call arguments.
fn last_array_literal(body: &str) -> Option<&str> {
    let bytes = body.as_bytes();
    let mut last: Option<&str> = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let mut depth = 0i32;
            let mut j = i;
            while j < bytes.len() {
                if bytes[j] == b'[' { depth += 1; }
                else if bytes[j] == b']' {
                    depth -= 1;
                    if depth == 0 {
                        last = Some(&body[i + 1..j]);
                        i = j + 1;
                        break;
                    }
                }
                j += 1;
            }
            if j == bytes.len() { break; }
        } else {
            i += 1;
        }
    }
    last
}

fn body_uses_router_navigation(body: &str) -> bool {
    // Require at least one of the router method patterns AND a `router`
    // identifier somewhere — to avoid catching unrelated `.push(` on arrays.
    if !body.contains("router") { return false; }
    ROUTER_METHODS.iter().any(|p| body.contains(p))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let bytes = ctx.source.as_bytes();
        let mut search_from = 0;
        while let Some(rel) = ctx.source[search_from..].find("useEffect(") {
            let abs = search_from + rel;
            let paren = abs + "useEffect".len();
            let Some(close) = find_matching_paren(bytes, paren) else { break };
            let body = &ctx.source[paren + 1..close];
            let Some(deps) = last_array_literal(body) else {
                search_from = close + 1;
                continue;
            };
            // Empty deps: trim whitespace.
            if !deps.trim().is_empty() {
                search_from = close + 1;
                continue;
            }
            if !body_uses_router_navigation(body) {
                search_from = close + 1;
                continue;
            }
            let prefix = &ctx.source[..abs];
            let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
            let col = prefix.rfind('\n').map_or(abs, |nl| abs - nl - 1) + 1;
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: col,
                rule_id: super::META.id.into(),
                message: "`router.push(...)` in a mount-only `useEffect` always navigates on first render. \
                          Move it into an event handler, gate it on a condition, or use a server-side redirect."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
            search_from = close + 1;
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
    fn flags_router_push_empty_deps() {
        let src = "useEffect(() => { router.push('/dashboard'); }, []);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_router_push_with_deps() {
        let src = "useEffect(() => { if (auth) router.push('/d'); }, [auth]);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_router_push_outside_effect() {
        let src = "function go() { router.push('/dashboard'); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_useeffect_without_router() {
        let src = "useEffect(() => { setData(42); }, []);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_router_replace_empty_deps() {
        let src = "useEffect(() => { router.replace('/login'); }, []);";
        assert_eq!(run(src).len(), 1);
    }
}
