//! svelte-no-effect-for-derived — text backend.
//!
//! Detects `$effect(() => { name = expr; })` blocks whose body is a single
//! assignment to a previously-declared identifier — the canonical anti-pattern
//! that should be a `$derived` instead.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_svelte(path: &std::path::Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("svelte")
}

/// Walk forward from `start` and return the byte index just past the matching
/// closing brace of the `{`-opened block at `start`. Returns `None` if no
/// match is found in `bytes`.
fn matching_brace_end(bytes: &[u8], start: usize) -> Option<usize> {
    debug_assert_eq!(bytes.get(start), Some(&b'{'));
    let mut depth: i32 = 0;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Inspect the body of a `$effect(() => { ... })` block (without the outer
/// braces) and return true if it looks like a single bare assignment to a
/// pre-declared variable: `ident = expr;` (or `ident.x = expr;` etc.).
fn body_is_assignment(body: &str) -> bool {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Reject obvious side-effect calls and control flow.
    if trimmed.starts_with("if ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("return")
        || trimmed.starts_with("//")
    {
        return false;
    }
    // Must contain a single `=` that isn't part of `==`, `!=`, `<=`, `>=`, `=>`.
    let bytes = trimmed.as_bytes();
    let mut eq_idx: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'=' {
            let prev = if i > 0 { bytes[i - 1] } else { b' ' };
            let next = bytes.get(i + 1).copied().unwrap_or(b' ');
            let is_compare = next == b'=' || prev == b'=' || prev == b'!' || prev == b'<' || prev == b'>';
            let is_arrow = next == b'>';
            if !is_compare && !is_arrow {
                if eq_idx.is_some() {
                    // multiple top-level assignments → not the simple case
                    return false;
                }
                eq_idx = Some(i);
            }
        }
        i += 1;
    }
    let Some(idx) = eq_idx else { return false };
    let lhs = trimmed[..idx].trim();
    if lhs.is_empty() {
        return false;
    }
    // LHS must be an identifier (or member access) — no parens, no braces.
    if lhs.contains('(') || lhs.contains('{') || lhs.contains('[') {
        return false;
    }
    // First char of LHS must look like an identifier start.
    let first = lhs.as_bytes()[0];
    if !(first.is_ascii_alphabetic() || first == b'_' || first == b'$') {
        return false;
    }
    true
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_svelte(ctx.path) {
            return Vec::new();
        }
        let source = ctx.source;
        let bytes = source.as_bytes();
        let mut diagnostics = Vec::new();
        let mut search_from = 0;
        while let Some(rel) = source[search_from..].find("$effect") {
            let start = search_from + rel;
            // Skip identifier collisions like `my$effect`.
            if start > 0 {
                let prev = bytes[start - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'$' {
                    search_from = start + "$effect".len();
                    continue;
                }
            }
            // Find `(` after `$effect`.
            let after = start + "$effect".len();
            let Some(paren_rel) = source[after..].find('(') else { break };
            let paren = after + paren_rel;
            // Find arrow `=>` then `{`.
            let Some(arrow_rel) = source[paren..].find("=>") else {
                search_from = paren + 1;
                continue;
            };
            let arrow = paren + arrow_rel;
            let Some(brace_rel) = source[arrow..].find('{') else {
                search_from = arrow + 2;
                continue;
            };
            let brace = arrow + brace_rel;
            let Some(end) = matching_brace_end(bytes, brace) else {
                break;
            };
            let body = &source[brace + 1..end - 1];
            if body_is_assignment(body) {
                let line = source[..start].bytes().filter(|b| *b == b'\n').count() + 1;
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column: 1,
                    rule_id: "svelte-no-effect-for-derived".into(),
                    message: "Use `$derived` instead of `$effect` to compute a value from reactive state.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            search_from = end;
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.svelte"), source))
    }

    #[test]
    fn flags_effect_with_assignment() {
        let src = "<script>\nlet count = $state(0);\nlet doubled;\n$effect(() => { doubled = count * 2; });\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiline_effect_assignment() {
        let src = "<script>\n$effect(() => {\n  total = a + b;\n});\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_derived() {
        let src = "<script>\nlet doubled = $derived(count * 2);\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_effect_with_side_effect() {
        let src = "<script>\n$effect(() => { console.log(count); });\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_svelte_files() {
        let src = "$effect(() => { x = y; });";
        let diags = Check.check(&CheckCtx::for_test(Path::new("a.ts"), src));
        assert!(diags.is_empty());
    }
}
