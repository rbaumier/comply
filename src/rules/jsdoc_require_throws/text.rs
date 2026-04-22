//! jsdoc/require-throws — functions with a visible throw path must document `@throws`.
//!
//! TS / JS / TSX heuristic: JSDoc block immediately followed by a function
//! (sync or async, statement or expression) whose body contains a `throw`
//! statement, with no `@throws` tag in the block.
//!
//! Rust heuristic: `///` doc comment block immediately followed by a `fn`
//! signature whose body contains `panic!` / `unwrap()` / `expect(` with no
//! `# Errors` or `# Panics` section. This is intentionally narrow to keep
//! false positives down — we do NOT flag every fn returning `Result`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, following_code, has_tag, parse_tags};

#[derive(Debug)]
pub struct Check;

fn is_rust_path(ctx: &CheckCtx) -> bool {
    ctx.path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e == "rs")
        .unwrap_or(false)
}

// ----- JS / TS side -----

fn starts_a_function(code: &str) -> bool {
    let first = code
        .lines()
        .map(str::trim_start)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    first.starts_with("function ")
        || first.starts_with("async function ")
        || first.starts_with("export function ")
        || first.starts_with("export async function ")
        || first.starts_with("export default function ")
        || first.starts_with("export default async function ")
        || first.starts_with("const ")
        || first.starts_with("let ")
        || first.starts_with("export const ")
        || first.starts_with("export let ")
}

fn has_throw(code: &str) -> bool {
    code.contains("throw ")
}

fn check_js(ctx: &CheckCtx) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for block in find_jsdoc_blocks(ctx.source) {
        let tags = parse_tags(&block.content);
        if has_tag(&tags, "throws") {
            continue;
        }
        let code = following_code(ctx.source, block.raw);
        if !starts_a_function(code) {
            continue;
        }
        if !has_throw(code) {
            continue;
        }
        out.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: block.start_line + 1,
            column: 1,
            rule_id: "jsdoc/require-throws".into(),
            message: "Function contains `throw` — document it with `@throws {ErrorType} when ...`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
    out
}

// ----- Rust side -----

/// Returns (start_line, block_content_lowercased, line_after_block).
fn find_rust_doc_blocks(source: &str) -> Vec<(usize, String, usize)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("///") {
            let start = i;
            let mut body = String::new();
            while i < lines.len() && lines[i].trim_start().starts_with("///") {
                let l = lines[i].trim_start().trim_start_matches("///").trim();
                body.push_str(l);
                body.push('\n');
                i += 1;
            }
            out.push((start, body.to_lowercase(), i));
        } else {
            i += 1;
        }
    }
    out
}

fn rust_fn_has_fallible_body(source: &str, from_line: usize) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    // Skip attribute lines (#[...]) before the fn signature.
    let mut i = from_line;
    while i < lines.len() {
        let t = lines[i].trim_start();
        if t.starts_with("#[") || t.is_empty() {
            i += 1;
            continue;
        }
        break;
    }
    if i >= lines.len() {
        return false;
    }
    let sig = lines[i].trim_start();
    let is_fn = sig.starts_with("fn ")
        || sig.starts_with("pub fn ")
        || sig.starts_with("pub(crate) fn ")
        || sig.starts_with("pub(super) fn ")
        || sig.starts_with("async fn ")
        || sig.starts_with("pub async fn ")
        || sig.starts_with("unsafe fn ")
        || sig.starts_with("pub unsafe fn ");
    if !is_fn {
        return false;
    }
    // Grab up to ~40 following lines to peek at the body.
    let end = (i + 40).min(lines.len());
    let slice = lines[i..end].join("\n");
    slice.contains("panic!") || slice.contains(".unwrap()") || slice.contains(".expect(")
}

fn check_rust(ctx: &CheckCtx) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (start_line, body_lc, next_line) in find_rust_doc_blocks(ctx.source) {
        if body_lc.contains("# errors") || body_lc.contains("# panics") {
            continue;
        }
        if !rust_fn_has_fallible_body(ctx.source, next_line) {
            continue;
        }
        out.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: start_line + 1,
            column: 1,
            rule_id: "jsdoc/require-throws".into(),
            message: "Function may panic — add a `# Panics` or `# Errors` section to the doc comment.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
    out
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if is_rust_path(ctx) {
            check_rust(ctx)
        } else {
            check_js(ctx)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }
    fn run_rs(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.rs"), source))
    }

    #[test]
    fn ts_flags_throwing_fn_without_throws() {
        let src = "/**\n * does x\n */\nfunction f() { throw new Error('x'); }";
        assert_eq!(run_ts(src).len(), 1);
    }

    #[test]
    fn ts_allows_throwing_fn_with_throws() {
        let src = "/**\n * does x\n * @throws {Error} when broken\n */\nfunction f() { throw new Error('x'); }";
        assert!(run_ts(src).is_empty());
    }

    #[test]
    fn ts_allows_fn_without_throw() {
        let src = "/**\n * ok\n */\nfunction f() { return 1; }";
        assert!(run_ts(src).is_empty());
    }

    #[test]
    fn rs_flags_panicking_fn_without_panics_section() {
        let src = "/// does x\nfn f() { panic!(\"oops\"); }";
        assert_eq!(run_rs(src).len(), 1);
    }

    #[test]
    fn rs_allows_panicking_fn_with_panics_section() {
        let src = "/// does x\n///\n/// # Panics\n/// when broken\nfn f() { panic!(\"oops\"); }";
        assert!(run_rs(src).is_empty());
    }

    #[test]
    fn rs_allows_non_panicking_fn() {
        let src = "/// does x\nfn f() -> i32 { 1 }";
        assert!(run_rs(src).is_empty());
    }
}
