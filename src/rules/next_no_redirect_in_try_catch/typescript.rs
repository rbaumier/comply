//! Flag `redirect(` calls that appear inside a `try { ... }` block. The
//! detection is text-based: track when we're inside a `try { ... }` body via
//! a brace counter, and flag any `redirect(` we see while the counter is > 0.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

fn imports_next_redirect(source: &str) -> bool {
    // Only fire when `redirect` is imported from `next/navigation`.
    for line in source.lines() {
        let t = line.trim_start();
        if !t.starts_with("import ") {
            continue;
        }
        if !(t.contains("'next/navigation'") || t.contains("\"next/navigation\"")) {
            continue;
        }
        if t.contains("redirect") {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["next/navigation"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !imports_next_redirect(ctx.source) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();

        // Single-pass walk over the source. Track:
        // - `depth`: current brace depth.
        // - `pending_try`: true when we just consumed `try` and are looking for
        //   the next `{` to open the try body.
        // - `try_stack`: depths-inside-the-try-body for each open try frame.
        let bytes = ctx.source.as_bytes();
        let mut depth: i32 = 0;
        let mut pending_try = false;
        let mut try_stack: Vec<i32> = Vec::new();
        // Cheap line/col tracking — increment as we walk.
        let mut line: usize = 1;
        let mut col: usize = 1;
        // Running state to skip line comments. Block comments are rare in
        // try/catch bodies and not worth handling for a heuristic rule.
        let mut in_line_comment = false;

        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'\n' {
                in_line_comment = false;
                line += 1;
                col = 1;
                i += 1;
                continue;
            }
            if in_line_comment {
                i += 1;
                col += 1;
                continue;
            }
            if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                in_line_comment = true;
                i += 2;
                col += 2;
                continue;
            }

            // Detect `try` keyword (word-boundary on both sides).
            if b == b't'
                && bytes[i..].len() >= 3
                && &bytes[i..i + 3] == b"try"
                && (i == 0 || (!bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_'))
                && (i + 3 >= bytes.len()
                    || (!bytes[i + 3].is_ascii_alphanumeric() && bytes[i + 3] != b'_'))
            {
                pending_try = true;
                i += 3;
                col += 3;
                continue;
            }

            match b {
                b'{' => {
                    depth += 1;
                    if pending_try {
                        try_stack.push(depth);
                        pending_try = false;
                    }
                }
                b'}' => {
                    while let Some(&top) = try_stack.last() {
                        if depth <= top {
                            try_stack.pop();
                        } else {
                            break;
                        }
                    }
                    depth -= 1;
                }
                _ => {
                    // Any non-whitespace token cancels a pending `try` — e.g.
                    // `try` used as an identifier suffix is unusual, but better
                    // to drop the pending state than to mis-attribute.
                    if !b.is_ascii_whitespace() && pending_try && b != b'/' {
                        pending_try = false;
                    }
                }
            }

            // Look for `redirect(` starting here. Only flag if any try frame
            // is currently open AND this is a word-boundary identifier.
            if !try_stack.is_empty()
                && bytes[i..].len() >= 9
                && &bytes[i..i + 9] == b"redirect("
                && (i == 0 || (!bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_'))
            {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column: col,
                    rule_id: super::META.id.into(),
                    message: "`redirect()` inside `try { ... }` is swallowed by the catch — \
                              Next.js relies on a thrown error for control flow. Move it \
                              outside the try, or rethrow in catch."
                        .to_string(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            i += 1;
            col += 1;
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("page.ts"), source))
    }

    #[test]
    fn flags_redirect_in_try_block() {
        let src = "import { redirect } from 'next/navigation';\n\
                   async function f() { try { redirect('/login'); } catch (e) {} }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_redirect_in_nested_try() {
        let src = "import { redirect } from 'next/navigation';\n\
                   async function f() { if (x) { try { if (y) { redirect('/'); } } catch {} } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_redirect_outside_try() {
        let src = "import { redirect } from 'next/navigation';\n\
                   async function f() { try { doThing(); } catch (e) {} redirect('/login'); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_files_without_next_import() {
        let src =
            "function redirect(_: string) {}\nfunction f() { try { redirect('/x'); } catch {} }";
        assert!(run(src).is_empty());
    }
}
