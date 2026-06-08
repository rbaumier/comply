use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const WINDOW: usize = 4096;

fn is_awaited_at(window: &str, abs: usize) -> bool {
    let bytes = window.as_bytes();
    let mut i = abs;
    loop {
        while i > 0 {
            let c = bytes[i - 1];
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' {
                i -= 1;
            } else {
                break;
            }
        }
        if i >= 1 && bytes[i - 1] == b'.' {
            i -= 1;
            if i >= 1 && bytes[i - 1] == b'?' {
                i -= 1;
            }
            continue;
        }
        break;
    }
    let prefix = &window[..i];
    prefix.trim_end().ends_with("await")
}

fn has_unawaited_prefetch_before(source: &str, dehydrate_offset: usize) -> bool {
    let mut start = dehydrate_offset.saturating_sub(WINDOW);
    while start > 0 && !source.is_char_boundary(start) {
        start -= 1;
    }
    let window = &source[start..dehydrate_offset];
    let mut from = 0usize;
    while let Some(rel) = window[from..].find("prefetchQuery(") {
        let abs = from + rel;
        if is_awaited_at(window, abs) {
            from = abs + 1;
            continue;
        }
        if window[..abs].contains("await Promise.all(") {
            from = abs + 1;
            continue;
        }
        return true;
    }
    false
}

fn find_offenses(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = source[from..].find("dehydrate(") {
        let abs = from + rel;
        let pre = source.as_bytes().get(abs.saturating_sub(1)).copied();
        let is_boundary = pre.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'$');
        if is_boundary && has_unawaited_prefetch_before(source, abs) {
            out.push(abs);
        }
        from = abs + 1;
    }
    out
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["dehydrate"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.source_contains("dehydrate(") || !ctx.source_contains("prefetchQuery(") {
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
                    message: "`dehydrate(...)` runs before an `await prefetchQuery(...)` — \
                              pending queries serialize empty."
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
    fn flags_unawaited_prefetch() {
        let src =
            "queryClient.prefetchQuery({ queryKey: ['x'] }); const state = dehydrate(queryClient);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_awaited_prefetch() {
        let src = "await queryClient.prefetchQuery({ queryKey: ['x'] }); const state = dehydrate(queryClient);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_when_no_dehydrate() {
        let src = "queryClient.prefetchQuery({ queryKey: ['x'] });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_promise_all_awaited() {
        let src = "await Promise.all([queryClient.prefetchQuery({ queryKey: ['x'] }), queryClient.prefetchQuery({ queryKey: ['y'] })]); const state = dehydrate(queryClient);";
        assert!(run(src).is_empty());
    }
}
