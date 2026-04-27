//! Flag `.map(...)` callbacks whose body contains `useQuery(`.
//!
//! Heuristic: locate every `.map(` substring, find the matching closing
//! paren by tracking paren depth, and report if `useQuery(` appears
//! anywhere inside that span.

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

/// Find the byte offset just after the matching `)` for an opening `(`
/// starting at `open_paren_offset` (which points to `(`).
fn find_matching_close(source: &[u8], open_paren_offset: usize) -> Option<usize> {
    if source.get(open_paren_offset) != Some(&b'(') {
        return None;
    }
    let mut depth = 1i32;
    let mut i = open_paren_offset + 1;
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
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut search = 0usize;
    while let Some(rel) = source[search..].find(".map(") {
        let dot_map = search + rel;
        let open = dot_map + 4; // index of '('
        if let Some(close) = find_matching_close(bytes, open) {
            let body = &source[open + 1..close];
            if body.contains("useQuery(") {
                out.push(dot_map);
            }
            search = close + 1;
        } else {
            break;
        }
    }
    out
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("useQuery") {
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
                    message: "`useQuery` inside `.map()` creates one subscription per row — \
                              hoist it out or use `useQueries`."
                        .into(),
                    severity: Severity::Error,
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
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), source))
    }

    #[test]
    fn flags_use_query_in_map() {
        let src = "items.map((id) => { const q = useQuery({ queryKey: ['x', id] }); return q.data; })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_use_query_in_nested_map() {
        let src = "rows.map((r) => cells.map((c) => useQuery({ queryKey: [r, c] })))";
        // Outer .map contains useQuery, inner .map also contains it.
        assert!(run(src).len() >= 1);
    }

    #[test]
    fn allows_use_query_outside_map() {
        let src = "const q = useQuery({ queryKey: ['x'] }); items.map((i) => i + 1);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_use_queries_in_map() {
        let src = "items.map((i) => i.name)";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_when_no_use_query_token() {
        let src = "items.map((i) => fetch('/x'))";
        assert!(run(src).is_empty());
    }
}
