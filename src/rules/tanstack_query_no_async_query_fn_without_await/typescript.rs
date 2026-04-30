//! Find each `queryFn: async` and inspect a small window after it for
//! `fetch(` not preceded by `await`.

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

/// Find the matching `)` for an opening `(` at `open_offset`.
fn matching_paren_end(source: &[u8], open_offset: usize) -> Option<usize> {
    if source.get(open_offset) != Some(&b'(') {
        return None;
    }
    let mut depth = 1i32;
    let mut i = open_offset + 1;
    while i < source.len() {
        match source[i] {
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

fn find_offenses(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = source[from..].find("queryFn:") {
        let key_start = from + rel;
        let after = key_start + "queryFn:".len();
        let rest = source[after..].trim_start();
        if !rest.starts_with("async") {
            from = after;
            continue;
        }
        // Locate the function body: the next `{` after async args, or arrow
        // returning expression.
        let async_start = source.len() - rest.len();
        // Walk forward from async to find either `{` (block body) or `=>`
        // followed by a fetch expression. Look only within ~2KB.
        let mut limit = (async_start + 2048).min(source.len());
        while limit < source.len() && !source.is_char_boundary(limit) {
            limit += 1;
        }
        let window = &source[async_start..limit];
        // Locate the body start: after `=>` or after the first `{`.
        let body_start_rel = window.find("=>").map(|p| p + 2).or_else(|| window.find('{'));
        let Some(body_off) = body_start_rel else {
            from = after;
            continue;
        };
        let body_abs = async_start + body_off;
        // Body extent: if previous char was `{`, find matching `}`.
        let body_end = if bytes.get(body_abs.saturating_sub(1)) == Some(&b'{') {
            // Find matching closing brace.
            let mut depth = 1i32;
            let mut i = body_abs;
            while i < bytes.len() && depth > 0 {
                match bytes[i] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    _ => {}
                }
                i += 1;
            }
            i
        } else {
            // Arrow expression — bounded by next `,` or `}` at depth 0.
            let mut depth_curly = 0i32;
            let mut depth_paren = 0i32;
            let mut i = body_abs;
            while i < bytes.len() {
                match bytes[i] {
                    b'{' => depth_curly += 1,
                    b'}' => {
                        if depth_curly == 0 {
                            break;
                        }
                        depth_curly -= 1;
                    }
                    b'(' => depth_paren += 1,
                    b')' => {
                        if depth_paren == 0 {
                            break;
                        }
                        depth_paren -= 1;
                    }
                    b',' if depth_curly == 0 && depth_paren == 0 => break,
                    _ => {}
                }
                i += 1;
            }
            i
        };
        let body = &source[body_abs..body_end];
        // Find `fetch(` not preceded by `await `.
        let mut bf = 0usize;
        let mut flagged = false;
        while let Some(p) = body[bf..].find("fetch(") {
            let pos = bf + p;
            let prefix = &body[..pos];
            let stripped = prefix.trim_end();
            if !stripped.ends_with("await") {
                // Verify it's a standalone fetch( call, not e.g. `prefetch(`.
                let pre_char = body.as_bytes().get(pos.saturating_sub(1)).copied();
                let is_word_boundary = pre_char.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'$');
                if is_word_boundary {
                    out.push(body_abs + pos);
                    flagged = true;
                    break;
                }
            }
            bf = pos + 1;
        }
        let _ = matching_paren_end; // silence unused if heuristics evolve
        let _ = flagged;
        from = body_end;
    }
    out
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["queryFn"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("queryFn") {
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
                    message: "`queryFn: async` returns `fetch(...)` without `await` — \
                              the query resolves with an unconsumed Response."
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
    fn flags_async_arrow_returning_fetch() {
        let src = "useQuery({ queryFn: async () => fetch('/api/x'), queryKey: ['x'] })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_async_block_returning_fetch() {
        let src = "useQuery({ queryFn: async () => { return fetch('/api/x'); } })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_awaited_fetch() {
        let src = "useQuery({ queryFn: async () => { const r = await fetch('/x'); return r.json(); } })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_inline_awaited_fetch() {
        let src = "useQuery({ queryFn: async () => (await fetch('/x')).json() })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_query_files() {
        let src = "const x = () => fetch('/x');";
        assert!(run(src).is_empty());
    }
}
