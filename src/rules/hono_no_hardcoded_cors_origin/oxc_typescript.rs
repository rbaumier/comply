use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

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

fn is_hono_cors(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "hono/cors")
        || (crate::oxc_helpers::source_contains(source, "hono")
            && crate::oxc_helpers::source_contains(source, "cors("))
}

fn find_origin_literal(body: &str) -> Option<usize> {
    let key = "origin:";
    let mut from = 0usize;
    while let Some(rel) = body[from..].find(key) {
        let after = from + rel + key.len();
        let rest = &body[after..];
        let trimmed_skip = rest.len() - rest.trim_start().len();
        let value_start = after + trimmed_skip;
        let first = body.as_bytes().get(value_start).copied();
        match first {
            Some(b'"') | Some(b'\'') => return Some(value_start),
            Some(b'[') => {
                let arr = &body[value_start..];
                if arr.starts_with('[') && arr[1..].trim_start().starts_with(['"', '\'']) {
                    return Some(value_start);
                }
            }
            _ => {}
        }
        from = after;
    }
    None
}

fn find_hardcoded_origins(source: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut search_from = 0usize;
    while let Some(rel) = source[search_from..].find("cors(") {
        let start = search_from + rel;
        let after = start + "cors(".len();
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
        if let Some(origin_pos) = find_origin_literal(body) {
            let abs = after + origin_pos;
            let (line, col) = byte_to_line_col(source, abs);
            out.push((line, col));
        }
        search_from = i;
    }
    out
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["hono"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_hono_cors(ctx.source) {
            return Vec::new();
        }
        find_hardcoded_origins(ctx.source)
            .into_iter()
            .map(|(line, column)| Diagnostic {
                path: Arc::clone(&ctx.path_arc),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_string_literal_origin() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors({ origin: 'https://example.com' }));";
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
