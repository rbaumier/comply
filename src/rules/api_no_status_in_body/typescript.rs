//! Look for `status: 2`, `status: 4`, `status: 5` (HTTP code-shaped
//! values) in a context that looks like a response body. The line
//! either starts with `return` or sits inside `c.json(`, `res.json(`,
//! or `Response.json(` (Hono / Express / Web).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const ROUTE_HINTS: &[&str] = &["route", "api", "handler", "controller", "endpoint"];

fn looks_like_api_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().to_ascii_lowercase();
    ROUTE_HINTS.iter().any(|h| s.contains(h))
}

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

const PREFIXES: &[&str] = &["status: 2", "status: 3", "status: 4", "status: 5"];

fn line_starts_with_response_context(source: &str, offset: usize) -> bool {
    // Walk back to the previous newline.
    let prev_nl = source[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line = &source[prev_nl..offset];
    // Look at the broader scope: ~500 chars back for a return statement
    // or response method invocation.
    let mut look_start = offset.saturating_sub(500);
    while look_start > 0 && !source.is_char_boundary(look_start) {
        look_start -= 1;
    }
    let scope = &source[look_start..offset];
    let scope_signals = [
        "return ",
        "return{",
        ".json(",
        "Response.json(",
        "c.json(",
        "res.json(",
        "res.send(",
    ];
    let line_or_scope_match = line.contains("return")
        || scope_signals.iter().any(|s| scope.contains(s));
    line_or_scope_match
}

fn find_offenses(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for prefix in PREFIXES {
        let mut from = 0usize;
        while let Some(rel) = source[from..].find(prefix) {
            let abs = from + rel;
            // Ensure word boundary before "status".
            let pre = source.as_bytes().get(abs.saturating_sub(1)).copied();
            let pre_ok = pre.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'$');
            if pre_ok && line_starts_with_response_context(source, abs) {
                // Extra: need to read 3 digits after "status: " to distinguish
                // 200 from 2 (timeout-ish). Look at ~5 chars ahead.
                let after = &source[abs + "status: ".len()..];
                let bs = after.as_bytes();
                let mut k = 0usize;
                while k < bs.len() && bs[k].is_ascii_digit() {
                    k += 1;
                }
                if k >= 3 {
                    out.push(abs);
                }
            }
            from = abs + prefix.len();
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> { Some(&["status:"]) }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !looks_like_api_path(ctx.path) {
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
                    message: "HTTP status code in response body — set the response status instead and drop the field."
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

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_status_200_in_return() {
        let src = "function GET() { return { status: 200, data: 1 }; }";
        assert_eq!(run_at(src, "src/routes/x.ts").len(), 1);
    }

    #[test]
    fn flags_status_400_in_c_json() {
        let src = "app.get('/x', (c) => c.json({ status: 400, error: 'bad' }))";
        assert_eq!(run_at(src, "src/api/x.ts").len(), 1);
    }

    #[test]
    fn allows_status_outside_api_files() {
        let src = "return { status: 200, data: 1 };";
        assert!(run_at(src, "src/lib/util.ts").is_empty());
    }

    #[test]
    fn allows_status_short_value() {
        // `status: 2` alone (2-char code) probably means a state, not HTTP.
        let src = "return { status: 2, data: 1 };";
        assert!(run_at(src, "src/routes/x.ts").is_empty());
    }
}
