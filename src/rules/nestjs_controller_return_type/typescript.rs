//! Flag controller methods (in classes decorated with `@Controller`) that
//! declare an `async` method without an explicit return-type annotation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

fn is_nestjs_controller_file(source: &str) -> bool {
    source.contains("@Controller")
}

/// Walk the source character-by-character, find every `async ` keyword that
/// is followed by an identifier and a `(...)` argument list, then check if a
/// `:` (return-type annotation) immediately follows the closing `)`. We skip
/// `async function` (top-level functions), `async (` (arrow/IIFE), and
/// `async => ...` (arrow shorthand) since those aren't controller methods.
fn flag_offsets(source: &str) -> Vec<usize> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 6 <= bytes.len() {
        // Match `async ` with word-boundary on the left.
        if &bytes[i..i + 6] == b"async "
            && (i == 0 || (!bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_'))
        {
            let after_async = i + 6;
            // Skip leading whitespace.
            let mut j = after_async;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            // Reject `async function`.
            if source[j..].starts_with("function") {
                i = j + 8;
                continue;
            }
            // Need an identifier (method name) starting here.
            let name_start = j;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j == name_start {
                // No identifier — likely `async (...)` arrow. Skip.
                i = after_async;
                continue;
            }
            // Optional generic params `<T>` — skip a single balanced `<...>`.
            // We tolerate this best-effort; real code is `async fn<T>(...)`.
            // Skip whitespace.
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            // Expect `(`.
            if j >= bytes.len() || bytes[j] != b'(' {
                i = after_async;
                continue;
            }
            // Find matching `)`.
            let mut depth = 0i32;
            let open_paren = j;
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
            let Some(close) = close_idx else {
                break;
            };
            // Skip whitespace after `)`.
            let mut k = close + 1;
            while k < bytes.len() && (bytes[k] == b' ' || bytes[k] == b'\t' || bytes[k] == b'\n') {
                k += 1;
            }
            // If next non-whitespace is `:` it's annotated; otherwise flag.
            if k >= bytes.len() || bytes[k] != b':' {
                out.push(name_start);
            }
            let _ = open_paren;
            i = close + 1;
        } else {
            i += 1;
        }
    }
    out
}

fn line_col_for_offset(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, b) in source.as_bytes().iter().enumerate() {
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
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@Controller"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_nestjs_controller_file(ctx.source) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for offset in flag_offsets(ctx.source) {
            let (line, col) = line_col_for_offset(ctx.source, offset);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: col,
                rule_id: super::META.id.into(),
                message: "Controller method has no explicit return type — annotate it with \
                          `: Promise<Dto>` / `: Observable<Dto>`."
                    .to_string(),
                severity: Severity::Warning,
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
        Check.check(&CheckCtx::for_test(Path::new("ctrl.ts"), source))
    }

    #[test]
    fn flags_async_method_without_return_type() {
        let src = "@Controller() class C { async create(@Body() dto: Dto) { return dto; } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_async_method_with_promise_return_type() {
        let src = "@Controller() class C { async create(@Body() dto: Dto): Promise<Dto> { return dto; } }";
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
