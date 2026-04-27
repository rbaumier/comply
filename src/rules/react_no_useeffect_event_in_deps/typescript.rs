//! Heuristic detection:
//! 1. Find names introduced by `const NAME = useEffectEvent(...)`.
//! 2. For each `useEffect(...)` call, look at the second argument (a
//!    `[ ... ]` array literal). If any of those names appear inside, flag.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

fn collect_event_names(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in source.lines() {
        // Match `const NAME = useEffectEvent(`
        let trimmed = line.trim_start();
        for kw in ["const ", "let ", "var "] {
            if !trimmed.starts_with(kw) { continue; }
            let after = &trimmed[kw.len()..];
            let Some(eq_idx) = after.find('=') else { continue };
            let name = after[..eq_idx].trim().trim_end_matches(':');
            // Strip type annotations: `name: T`.
            let name = name.split(':').next().unwrap_or("").trim();
            let rhs = after[eq_idx + 1..].trim_start();
            if rhs.starts_with("useEffectEvent(") {
                if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

fn find_matching_bracket(bytes: &[u8], start: usize, open: u8, close: u8) -> Option<usize> {
    debug_assert_eq!(bytes[start], open);
    let mut depth: i32 = 0;
    let mut i = start;
    while i < bytes.len() {
        let b = bytes[i];
        if b == open { depth += 1; }
        else if b == close {
            depth -= 1;
            if depth == 0 { return Some(i); }
        }
        i += 1;
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let names = collect_event_names(ctx.source);
        if names.is_empty() { return Vec::new(); }
        let mut diagnostics = Vec::new();
        let bytes = ctx.source.as_bytes();
        let mut from = 0;
        while let Some(rel) = ctx.source[from..].find("useEffect(") {
            let abs = from + rel;
            let paren = abs + "useEffect".len();
            let Some(close) = find_matching_bracket(bytes, paren, b'(', b')') else { break };
            let body = &ctx.source[paren + 1..close];
            // Find the deps array — the LAST `[...]` block in the call.
            let mut deps: Option<&str> = None;
            let body_bytes = body.as_bytes();
            let mut i = 0;
            while i < body_bytes.len() {
                if body_bytes[i] == b'[' {
                    if let Some(end) = find_matching_bracket(body_bytes, i, b'[', b']') {
                        deps = Some(&body[i + 1..end]);
                        i = end + 1;
                        continue;
                    }
                }
                i += 1;
            }
            let Some(deps) = deps else {
                from = close + 1;
                continue;
            };
            for name in &names {
                let parts: Vec<&str> = deps.split([',', ' ', '\t', '\n']).collect();
                if parts.iter().any(|p| p.trim() == name) {
                    let prefix = &ctx.source[..abs];
                    let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
                    let col = prefix.rfind('\n').map_or(abs, |nl| abs - nl - 1) + 1;
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column: col,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{name}` is from `useEffectEvent` and has a stable identity — listing it as \
                             a dependency is meaningless. Remove it from the deps array."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            from = close + 1;
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
    fn flags_event_in_deps() {
        let src = "const onTick = useEffectEvent(() => {});\nuseEffect(() => { onTick(); }, [onTick]);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_event_not_in_deps() {
        let src = "const onTick = useEffectEvent(() => {});\nuseEffect(() => { onTick(); }, []);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_when_no_event_event() {
        let src = "const fn = () => {};\nuseEffect(() => fn(), [fn]);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_event_among_other_deps() {
        let src = "const onTick = useEffectEvent(() => {});\nuseEffect(() => { onTick(); }, [count, onTick]);";
        assert_eq!(run(src).len(), 1);
    }
}
