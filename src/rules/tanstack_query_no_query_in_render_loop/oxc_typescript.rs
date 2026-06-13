use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

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
        let open = dot_map + 4;
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

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useQuery"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.source_contains("useQuery") {
            return Vec::new();
        }
        find_offenses(ctx.source)
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_use_query_in_map() {
        let src =
            "items.map((id) => { const q = useQuery({ queryKey: ['x', id] }); return q.data; })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_use_query_in_nested_map() {
        let src = "rows.map((r) => cells.map((c) => useQuery({ queryKey: [r, c] })))";
        assert!(run(src).len() >= 1);
    }

    #[test]
    fn allows_use_query_outside_map() {
        let src = "const q = useQuery({ queryKey: ['x'] }); items.map((i) => i + 1);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_when_no_use_query_token() {
        let src = "items.map((i) => fetch('/x'))";
        assert!(run(src).is_empty());
    }
}
