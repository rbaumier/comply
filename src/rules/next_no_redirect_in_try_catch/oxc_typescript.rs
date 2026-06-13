use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn imports_next_redirect(source: &str) -> bool {
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

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["next/navigation"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !imports_next_redirect(ctx.source) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let bytes = ctx.source.as_bytes();
        let mut depth: i32 = 0;
        let mut pending_try = false;
        let mut try_stack: Vec<i32> = Vec::new();
        let mut line: usize = 1;
        let mut col: usize = 1;
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
                    if !b.is_ascii_whitespace() && pending_try && b != b'/' {
                        pending_try = false;
                    }
                }
            }
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
                        .into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
