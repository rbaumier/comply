//! hono-no-get-with-body backend — flag body parsing inside GET/HEAD handlers.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const BODY_CALLS: &[&str] = &[
    "c.req.json(",
    "c.req.text(",
    "c.req.parseBody(",
    "c.req.formData(",
];

fn is_hono(source: &str) -> bool {
    source.contains("hono") || source.contains("Hono")
}

/// Find each `.get(...)` / `.head(...)` route handler and report any body
/// parsing calls inside its body. We scan the source from the opening `(`
/// of the handler until the matching `)`.
fn find_violations(source: &str) -> Vec<(usize, usize, &'static str, &'static str)> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();

    for verb in &[".get(", ".head("] {
        let verb_name = if *verb == ".get(" { "GET" } else { "HEAD" };
        let mut from = 0usize;
        while let Some(rel) = source[from..].find(*verb) {
            let start = from + rel;
            let after = start + verb.len();
            let mut depth = 1;
            let mut i = after;
            while i < bytes.len() && depth > 0 {
                match bytes[i] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                i += 1;
            }
            let mut body_end = i.saturating_sub(1);
            while body_end > after && !source.is_char_boundary(body_end) {
                body_end -= 1;
            }
            let body = &source[after..body_end];
            for call in BODY_CALLS {
                let mut search = 0usize;
                while let Some(p) = body[search..].find(call) {
                    let abs = after + search + p;
                    let (line, col) = byte_to_line_col(source, abs);
                    out.push((line, col, verb_name, *call));
                    search += p + call.len();
                }
            }
            from = i;
        }
    }
    out
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

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["hono", "Hono"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_hono(ctx.source) {
            return Vec::new();
        }
        find_violations(ctx.source)
            .into_iter()
            .map(|(line, column, verb, call)| Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "{verb} handler reads the request body via `{}` — {verb} requests have no body.",
                    call.trim_end_matches('(')
                ),
                severity: Severity::Warning,
                span: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_get_with_json_body() {
        let src = "import { Hono } from 'hono';\napp.get('/x', async (c) => { const b = await c.req.json(); return c.json(b); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_head_with_text() {
        let src = "import { Hono } from 'hono';\napp.head('/x', async (c) => { const b = await c.req.text(); return c.body(null); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_parse_body_in_get() {
        let src = "import { Hono } from 'hono';\napp.get('/x', async (c) => { await c.req.parseBody(); return c.text('ok'); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_get_with_query() {
        let src =
            "import { Hono } from 'hono';\napp.get('/x', (c) => c.json({ q: c.req.query('q') }));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_post_with_body() {
        let src = "import { Hono } from 'hono';\napp.post('/x', async (c) => { const b = await c.req.json(); return c.json(b); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "app.get('/x', async (c) => { await c.req.json(); });";
        assert!(run(src).is_empty());
    }
}
