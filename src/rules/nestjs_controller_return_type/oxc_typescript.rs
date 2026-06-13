use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_nestjs_controller_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@Controller")
}

fn flag_offsets(source: &str) -> Vec<usize> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 6 <= bytes.len() {
        if &bytes[i..i + 6] == b"async "
            && (i == 0 || (!bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_'))
        {
            let after_async = i + 6;
            let mut j = after_async;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if source[j..].starts_with("function") {
                i = j + 8;
                continue;
            }
            let name_start = j;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j == name_start {
                i = after_async;
                continue;
            }
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j >= bytes.len() || bytes[j] != b'(' {
                i = after_async;
                continue;
            }
            let mut depth = 0i32;
            let mut close_idx: Option<usize> = None;
            while j < bytes.len() {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            close_idx = Some(j);
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            let Some(close) = close_idx else { break };
            let mut k = close + 1;
            while k < bytes.len()
                && (bytes[k] == b' ' || bytes[k] == b'\t' || bytes[k] == b'\n')
            {
                k += 1;
            }
            if k >= bytes.len() || bytes[k] != b':' {
                out.push(name_start);
            }
            i = close + 1;
        } else {
            i += 1;
        }
    }
    out
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@Controller"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_nestjs_controller_file(ctx.source) {
            return Vec::new();
        }
        flag_offsets(ctx.source)
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Controller method has no explicit return type — annotate it with \
                              `: Promise<Dto>` / `: Observable<Dto>`."
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
    fn flags_async_method_without_return_type() {
        let src = "@Controller() class C { async create(@Body() dto: Dto) { return dto; } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_async_method_with_promise_return_type() {
        let src =
            "@Controller() class C { async create(@Body() dto: Dto): Promise<Dto> { return dto; } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_async_method_with_observable_return_type() {
        let src = "@Controller() class C { async list(): Observable<Dto[]> { return of([]); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_controller_files() {
        let src = "class Service { async run() { return 1; } }";
        assert!(run(src).is_empty());
    }
}
