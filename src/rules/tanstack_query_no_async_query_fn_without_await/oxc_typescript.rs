use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

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
        let async_start = source.len() - rest.len();
        let mut limit = (async_start + 2048).min(source.len());
        while limit < source.len() && !source.is_char_boundary(limit) {
            limit += 1;
        }
        let window = &source[async_start..limit];
        let body_start_rel = window.find("=>").map(|p| p + 2).or_else(|| window.find('{'));
        let Some(body_off) = body_start_rel else {
            from = after;
            continue;
        };
        let body_abs = async_start + body_off;
        let body_end = if bytes.get(body_abs.saturating_sub(1)) == Some(&b'{') {
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
        let mut bf = 0usize;
        while let Some(p) = body[bf..].find("fetch(") {
            let pos = bf + p;
            let prefix = &body[..pos];
            let stripped = prefix.trim_end();
            if !stripped.ends_with("await") {
                let pre_char = body.as_bytes().get(pos.saturating_sub(1)).copied();
                let is_word_boundary =
                    pre_char.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'$');
                if is_word_boundary {
                    out.push(body_abs + pos);
                    break;
                }
            }
            bf = pos + 1;
        }
        from = body_end;
    }
    out
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["queryFn"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.source_contains("queryFn") {
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
        let src =
            "useQuery({ queryFn: async () => { const r = await fetch('/x'); return r.json(); } })";
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
