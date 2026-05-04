//! hono-no-hardcoded-cors-origin backend — flag `cors({ origin: "..." })` literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// True if the file looks like a Hono file using the cors middleware.
fn is_hono_cors(source: &str) -> bool {
    source.contains("hono/cors") || (source.contains("hono") && source.contains("cors("))
}

/// Scan `source` for `cors({ ... origin: "..." ... })` style configurations.
/// Returns (line, column) pairs where a string-literal origin appears.
fn find_hardcoded_origins(source: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    // Find each `cors(` call and inspect its argument object up to the matching `)`.
    let bytes = source.as_bytes();
    let mut search_from = 0usize;
    while let Some(rel) = source[search_from..].find("cors(") {
        let start = search_from + rel;
        let after = start + "cors(".len();
        // Find balanced closing paren.
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
        let body = &source[after..i.saturating_sub(1)];
        // Look for `origin:` followed by a string literal (single or double quote, not backtick template with var).
        if let Some(origin_pos) = find_origin_literal(body) {
            // Compute absolute byte offset → line/col.
            let abs = after + origin_pos;
            let (line, col) = byte_to_line_col(source, abs);
            out.push((line, col));
        }
        search_from = i;
    }
    out
}

/// Inside a `cors(...)` body, locate `origin:` followed by a quoted string literal
/// (i.e. hardcoded). Returns the offset of the literal's opening quote, or None.
fn find_origin_literal(body: &str) -> Option<usize> {
    let key = "origin:";
    let mut from = 0usize;
    while let Some(rel) = body[from..].find(key) {
        let after = from + rel + key.len();
        // Skip whitespace.
        let rest = &body[after..];
        let trimmed_skip = rest.len() - rest.trim_start().len();
        let value_start = after + trimmed_skip;
        let first = body.as_bytes().get(value_start).copied();
        match first {
            Some(b'"') | Some(b'\'') => return Some(value_start),
            // Array of strings is also hardcoded — `origin: ["https://..."]`.
            Some(b'[') => {
                let arr = &body[value_start..];
                if arr.starts_with('[')
                    && arr[1..].trim_start().starts_with(['"', '\''])
                {
                    return Some(value_start);
                }
            }
            _ => {}
        }
        from = after;
    }
    None
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
        Some(&["hono"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_hono_cors(ctx.source) {
            return Vec::new();
        }
        find_hardcoded_origins(ctx.source)
            .into_iter()
            .map(|(line, column)| Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "CORS `origin` is a hardcoded string — read it from an environment variable instead."
                    .into(),
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
    fn flags_string_literal_origin() {
        let src =
            "import { cors } from 'hono/cors';\napp.use(cors({ origin: 'https://example.com' }));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_double_quoted_origin() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors({ origin: \"https://example.com\" }));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_array_literal_origin() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors({ origin: ['https://a.com', 'https://b.com'] }));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_env_origin() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors({ origin: env.CORS_ORIGIN }));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_variable_origin() {
        let src = "import { cors } from 'hono/cors';\nconst o = process.env.O;\napp.use(cors({ origin: o }));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "app.use(cors({ origin: 'https://example.com' }));";
        assert!(run(src).is_empty());
    }
}
