//! Flag `process.on('unhandledRejection', ...)` handlers whose body does not
//! call `process.exit`. Detection is text-based: locate the registration,
//! capture the handler body via brace counting, scan for `process.exit`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

/// Find every byte offset where `process.on('unhandledRejection'` starts.
/// Accepts both single and double quotes around the event name.
fn registration_offsets(src: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let needles = [
        "process.on('unhandledRejection'",
        "process.on(\"unhandledRejection\"",
    ];
    for needle in needles {
        let mut from = 0;
        while let Some(rel) = src[from..].find(needle) {
            out.push(from + rel);
            from += rel + needle.len();
        }
    }
    out
}

/// Slice from `start` (anchor at `process.on(`) to the matching closing `)`
/// of the registration call. Returns `(handler_body_text, end_offset)` if we
/// can identify a balanced `(...)`. Returns None on malformed input.
fn registration_call_slice(src: &str, start: usize) -> Option<&str> {
    let bytes = src.as_bytes();
    let open_paren = src[start..].find('(')? + start;
    let mut depth = 0i32;
    for (i, b) in bytes.iter().enumerate().skip(open_paren) {
        match *b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&src[open_paren..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

fn line_col_for_offset(src: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, b) in src.as_bytes().iter().enumerate() {
        if i == offset {
            break;
        }
        if *b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> { Some(&["unhandledRejection"]) }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("unhandledRejection") {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for start in registration_offsets(ctx.source) {
            let Some(call) = registration_call_slice(ctx.source, start) else {
                continue;
            };
            // The call slice covers `(...)` of `process.on(...)`. The handler
            // body is whatever is inside.
            if call.contains("process.exit") {
                continue;
            }
            let (line, col) = line_col_for_offset(ctx.source, start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: col,
                rule_id: super::META.id.into(),
                message: "`unhandledRejection` handler does not call `process.exit` — the \
                          process keeps running in an unknown state. Exit explicitly."
                    .to_string(),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("server.ts"), source))
    }

    #[test]
    fn flags_handler_without_exit() {
        let src = "process.on('unhandledRejection', (err) => { console.error(err); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_handler_with_process_exit() {
        let src = "process.on('unhandledRejection', (err) => { console.error(err); process.exit(1); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_double_quoted_event_with_exit() {
        let src = "process.on(\"unhandledRejection\", (err) => { process.exit(1); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_unrelated_event() {
        let src = "process.on('SIGTERM', () => {});";
        assert!(run(src).is_empty());
    }
}
